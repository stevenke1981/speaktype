"""Read-only, fail-closed guard for a CPU or CUDA release asset group."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen


GROUPS = {
    "cpu": ("speaktype-setup-windows-x64.exe", "speaktype-windows-x64.zip", "SHA256SUMS.txt"),
    "cuda": ("speaktype-cuda-setup-windows-x64.exe", "speaktype-cuda-windows-x64.zip", "SHA256SUMS-cuda.txt"),
    "vulkan": ("speaktype-vulkan-setup-windows-x64.exe", "speaktype-vulkan-windows-x64.zip", "SHA256SUMS-vulkan.txt"),
}


class PreflightError(Exception):
    pass


def stream_hash(stream):
    digest = hashlib.sha256()
    while True:
        block = stream.read(1024 * 1024)
        if not block:
            return digest.hexdigest()
        digest.update(block)


def validate_tag(tag, cargo_toml):
    package = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", cargo_toml)
    version = re.search(r'^version\s*=\s*"([^"]+)"\s*$', package[1], re.M) if package else None
    if not version or not re.fullmatch(r"v" + re.escape(version[1]) + r"(?:-[0-9A-Za-z][0-9A-Za-z.-]*)?", tag):
        raise PreflightError("Release tag does not match the Cargo package version")


def local_hashes(directory, group):
    names = GROUPS[group]
    hashes = {}
    for name in names:
        path = directory / name
        if not path.is_file():
            raise PreflightError("Missing local asset: " + name)
        with path.open("rb") as stream:
            hashes[name] = stream_hash(stream)
    manifest = {}
    for line in (directory / names[-1]).read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-fA-F]{64})  ([^/\\]+)", line)
        if not match or match[2] in manifest:
            raise PreflightError("Invalid or duplicate checksum entry")
        manifest[match[2]] = match[1].lower()
    if set(manifest) != set(names[:-1]):
        raise PreflightError("Checksum manifest must cover exactly this asset group")
    if any(manifest[name] != hashes[name] for name in names[:-1]):
        raise PreflightError("Local asset does not match its checksum manifest")
    return hashes


class GitHubAPI:
    """Only GET requests; downloads are public and never receive the API token."""

    def __init__(self, repo):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
            raise PreflightError("Invalid repository name")
        self.repo = repo

    def json(self, path):
        headers = {"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}
        if os.environ.get("GH_TOKEN"):
            headers["Authorization"] = "Bearer " + os.environ["GH_TOKEN"]
        with urlopen(Request("https://api.github.com/repos/" + self.repo + path, headers=headers), timeout=30) as response:
            return json.load(response)

    def assets(self, tag):
        try:
            release = self.json("/releases/tags/" + quote(tag, safe=""))
        except HTTPError as error:
            if error.code == 404:
                return []
            raise PreflightError("Release lookup failed (HTTP %s)" % error.code) from None
        result = []
        page = 1
        while True:
            assets = self.json("/releases/%s/assets?per_page=100&page=%s" % (int(release["id"]), page))
            result.extend(assets)
            if len(assets) < 100:
                return result
            page += 1

    def asset_hash(self, tag, name):
        url = "https://github.com/%s/releases/download/%s/%s" % (
            self.repo, quote(tag, safe=""), quote(name, safe=""))
        # No Authorization header: verify exactly what public users can download.
        with urlopen(url, timeout=120) as response:
            return stream_hash(response)


def preflight(api, tag, directory, group):
    expected = local_hashes(directory, group)
    assets = [asset for asset in api.assets(tag) if asset.get("name") in expected]
    if not assets:
        return "upload"
    names = [asset["name"] for asset in assets]
    if len(names) != len(set(names)):
        raise PreflightError("Duplicate remote asset names; refusing to publish")
    if set(names) != set(expected):
        raise PreflightError("Partial remote asset group; refusing to mix old and new files")
    for asset in assets:
        name = asset["name"]
        if asset.get("state") != "uploaded":
            raise PreflightError("Remote asset is not fully uploaded: " + name)
        if api.asset_hash(tag, name) != expected[name]:
            raise PreflightError("Remote/local content conflict: " + name)
    return "identical"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--group", choices=GROUPS, required=True)
    parser.add_argument("--directory", type=Path, default=Path("dist"))
    parser.add_argument("--cargo-toml", type=Path, default=Path("Cargo.toml"))
    args = parser.parse_args(argv)
    try:
        validate_tag(args.tag, args.cargo_toml.read_text(encoding="utf-8"))
        result = preflight(GitHubAPI(args.repo), args.tag, args.directory, args.group)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                output.write("upload=%s\n" % ("true" if result == "upload" else "false"))
        print("Release group %s: %s" % (args.group, result))
        return 0
    except PreflightError as error:
        print("Release preflight failed: " + str(error), file=sys.stderr)
    except Exception as error:
        # Network failures must never turn into permission to upload. Avoid
        # dumping request objects, headers, or authenticated URLs in CI logs.
        print("Release preflight failed (%s)" % type(error).__name__, file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
