import contextlib
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError


spec = importlib.util.spec_from_file_location("release_preflight", Path(__file__).parents[1] / "release_preflight.py")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def sha(data):
    return hashlib.sha256(data).hexdigest()


class FixtureAPI:
    def __init__(self, files):
        self.files = files
        self.downloads = []

    def assets(self, tag):
        return [{"name": name, "state": "uploaded"} for name in self.files]

    def asset_hash(self, tag, name):
        self.downloads.append(name)
        return sha(self.files[name])


class ReleasePreflightTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def local_group(self, group, suffix=b"A"):
        exe, archive, manifest = guard.GROUPS[group]
        files = {exe: b"fixture executable " + suffix, archive: b"fixture zip bytes " + suffix}
        files[manifest] = "".join("%s  %s\n" % (sha(data), name) for name, data in files.items()).encode("ascii")
        for name, data in files.items():
            (self.directory / name).write_bytes(data)
        return files

    def test_empty_group_allows_upload_and_ignores_other_group(self):
        for group in guard.GROUPS:
            with self.subTest(group=group):
                self.local_group(group)
                api = FixtureAPI({"unrelated.txt": b"old release data"})
                self.assertEqual(guard.preflight(api, "v0.5.2", self.directory, group), "upload")
                self.assertEqual(api.downloads, [])

    def test_complete_identical_group_downloads_every_file_and_noops(self):
        for group in guard.GROUPS:
            with self.subTest(group=group):
                files = self.local_group(group)
                api = FixtureAPI(files)
                self.assertEqual(guard.preflight(api, "v0.5.2", self.directory, group), "identical")
                self.assertCountEqual(api.downloads, files)

    def test_every_partial_group_is_rejected_before_downloading(self):
        for group in guard.GROUPS:
            files = self.local_group(group)
            names = list(files)
            for mask in range(1, 7):
                with self.subTest(group=group, mask=mask):
                    api = FixtureAPI({name: files[name] for i, name in enumerate(names) if mask & (1 << i)})
                    with self.assertRaisesRegex(guard.PreflightError, "Partial"):
                        guard.preflight(api, "v0.5.2", self.directory, group)
                    self.assertEqual(api.downloads, [])

    def test_rebuilt_zip_with_new_manifest_cannot_mix_with_old_partial_release(self):
        for group in guard.GROUPS:
            old = self.local_group(group, b"A")
            self.local_group(group, b"B")
            for kept in (guard.GROUPS[group][1], guard.GROUPS[group][2]):
                with self.subTest(group=group, kept=kept):
                    with self.assertRaisesRegex(guard.PreflightError, "Partial"):
                        guard.preflight(FixtureAPI({kept: old[kept]}), "v0.5.2", self.directory, group)

    def test_complete_old_release_conflicts_with_rebuilt_bytes(self):
        for group in guard.GROUPS:
            with self.subTest(group=group):
                old = self.local_group(group, b"A")
                self.local_group(group, b"B")
                with self.assertRaisesRegex(guard.PreflightError, "content conflict"):
                    guard.preflight(FixtureAPI(old), "v0.5.2", self.directory, group)

    def test_remote_checksum_or_binary_corruption_rejected(self):
        for name in guard.GROUPS["cpu"]:
            with self.subTest(name=name):
                files = self.local_group("cpu")
                files[name] += b"corrupt"
                with self.assertRaisesRegex(guard.PreflightError, "content conflict"):
                    guard.preflight(FixtureAPI(files), "v0.5.2", self.directory, "cpu")

    def test_invalid_local_group_never_queries_remote(self):
        for damage in ("missing", "wrong-hash", "duplicate", "extra", "wrong-group"):
            with self.subTest(damage=damage):
                files = self.local_group("cpu")
                exe, archive, manifest = guard.GROUPS["cpu"]
                if damage == "missing":
                    (self.directory / archive).unlink()
                elif damage == "wrong-hash":
                    (self.directory / archive).write_bytes(b"changed")
                else:
                    tail = files[manifest].splitlines(keepends=True)[0] if damage == "duplicate" else b"0" * 64 + b"  extra.zip\n"
                    (self.directory / manifest).write_bytes(tail if damage == "wrong-group" else files[manifest] + tail)
                api = FixtureAPI({})
                with patch.object(api, "assets", side_effect=AssertionError("must validate locally first")):
                    with self.assertRaises(guard.PreflightError):
                        guard.preflight(api, "v0.5.2", self.directory, "cpu")

    def test_duplicate_or_unfinished_remote_assets_rejected(self):
        files = self.local_group("cpu")
        api = FixtureAPI(files)
        assets = api.assets("v0.5.2")
        for invalid in (assets + assets[:1], [dict(asset, state="new") for asset in assets]):
            with self.subTest(assets=invalid), patch.object(api, "assets", return_value=invalid):
                with self.assertRaises(guard.PreflightError):
                    guard.preflight(api, "v0.5.2", self.directory, "cpu")

    def test_api_pagination_does_not_miss_later_assets(self):
        api = guard.GitHubAPI("owner/repo")
        first = [{"name": "other-%s" % i} for i in range(100)]
        last = [{"name": "SHA256SUMS.txt"}]
        with patch.object(api, "json", side_effect=[{"id": 7}, first, last]) as request:
            self.assertEqual(api.assets("v0.5.2"), first + last)
            self.assertEqual(request.call_args_list[-1].args[0], "/releases/7/assets?per_page=100&page=2")

    def test_only_release_lookup_404_counts_as_absent(self):
        api = guard.GitHubAPI("owner/repo")
        for code in (404, 403, 500):
            with self.subTest(code=code), patch.object(api, "json", side_effect=HTTPError("fixture", code, "error", {}, None)):
                if code == 404:
                    self.assertEqual(api.assets("v0.5.2"), [])
                else:
                    with self.assertRaises(guard.PreflightError):
                        api.assets("v0.5.2")

    def test_failure_never_emits_upload_permission(self):
        self.local_group("cpu")
        cargo = self.directory / "Cargo.toml"
        cargo.write_text('[package]\nname = "speaktype"\nversion = "0.5.2"\n', encoding="utf-8")
        output = self.directory / "output"
        for failure in (RuntimeError("network failure"), guard.PreflightError("partial")):
            with self.subTest(failure=failure), patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}), \
                    patch.object(guard, "preflight", side_effect=failure), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(guard.main(["--repo", "owner/repo", "--tag", "v0.5.2", "--group", "cpu",
                                             "--directory", str(self.directory), "--cargo-toml", str(cargo)]), 1)
                self.assertFalse(output.exists())

    def test_success_emits_exact_upload_decision(self):
        cargo = self.directory / "Cargo.toml"
        cargo.write_text('[package]\nversion = "0.5.2"\n', encoding="utf-8")
        for result, value in (("upload", "true"), ("identical", "false")):
            output = self.directory / result
            with patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}), patch.object(guard, "preflight", return_value=result), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(guard.main(["--repo", "owner/repo", "--tag", "v0.5.2", "--group", "cpu",
                                             "--cargo-toml", str(cargo)]), 0)
                self.assertEqual(output.read_text().strip(), "upload=" + value)

    def test_version_gate_accepts_matching_release_and_prerelease_only(self):
        cargo = '[package]\nname = "speaktype"\nversion = "0.5.2"\n[dependencies]\n'
        for tag in ("v0.5.2", "v0.5.2-test", "v0.5.2-rc.1"):
            guard.validate_tag(tag, cargo)
        for tag in ("v0.5.3", "v0.5.20", "v0.5.2oops", "v0.5.2-", "0.5.2"):
            with self.subTest(tag=tag), self.assertRaises(guard.PreflightError):
                guard.validate_tag(tag, cargo)


if __name__ == "__main__":
    unittest.main()
