; Inno Setup script for SpeakType (Windows x64).
; Build: ISCC.exe /DAppVersion=0.4.0 /DSrcDir=..\target\release installer\speaktype.iss
; Requires installer\ChineseTraditional.isl (fetched by scripts\build-installer.ps1).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SrcDir
  #define SrcDir "..\target\release"
#endif
#ifndef OutputName
  #define OutputName "speaktype-setup-windows-x64"
#endif
#define AppName "SpeakType"
#define AppExe "speaktype.exe"

[Setup]
AppId={{5E3B7F2A-9C41-4D8E-B1A6-7F0C2D9E8B34}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Ke Sheng Da
AppPublisherURL=https://github.com/stevenke1981/speaktype
AppSupportURL=https://github.com/stevenke1981/speaktype/issues
AppUpdatesURL=https://github.com/stevenke1981/speaktype/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename={#OutputName}
SetupIconFile=..\assets\speaktype.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName} {#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Admin (Program Files, all users) by default; "Install for me only" needs no admin.
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
AppMutex=Global\SpeakType.SingleInstance
CloseApplications=force
RestartApplications=no
ShowLanguageDialog=auto

[Languages]
Name: "chinesetraditional"; MessagesFile: "ChineseTraditional.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
chinesetraditional.TaskAutostart=登入 Windows 時自動啟動 SpeakType（常駐系統匣）
chinesetraditional.GroupStartup=啟動：
chinesetraditional.PurgeData=是否一併刪除 SpeakType 的設定、辨識紀錄、錄音檔與已下載的模型？%n%n（位置：%1）
english.TaskAutostart=Start SpeakType when I sign in to Windows (runs in the tray)
english.GroupStartup=Startup:
english.PurgeData=Also delete SpeakType settings, history, recordings and downloaded models?%n%n(Location: %1)

chinesetraditional.CudaDownload=正在下載 NVIDIA CUDA 元件（約 430 MB）…
english.CudaDownload=Downloading NVIDIA CUDA runtime (~430 MB)...
chinesetraditional.CudaFailed=CUDA 元件下載失敗。SpeakType 會先使用 CPU 辨識，之後可在「設定 → 辨識硬體」按「下載 CUDA 元件」重試。
english.CudaFailed=Downloading the CUDA runtime failed. SpeakType will use the CPU; retry later from Settings.

[Tasks]
Name: "autostart"; Description: "{cm:TaskAutostart}"; GroupDescription: "{cm:GroupStartup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SrcDir}\speaktype.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
#ifdef Cuda
Source: "cuda-libs.ps1"; DestDir: "{app}"; Flags: ignoreversion
#endif
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.en.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; Same per-user Run value the app's own "launch on startup" switch uses, so the
; checkbox in Settings and the installer stay in sync.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "SpeakType"; ValueData: """{app}\{#AppExe}"" --tray"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
#ifdef Cuda
; Fetch NVIDIA CUDA 12.6 runtime DLLs (~430 MB, SHA256-verified) into {app}.
; Visible window shows progress; failure keeps the app usable on the CPU.
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\cuda-libs.ps1"" -Dest ""{app}"""; StatusMsg: "{cm:CudaDownload}"; Flags: waituntilterminated; Check: not WizardSilent or CudaInSilent; AfterInstall: CudaResult
#endif
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#AppExe}"; Flags: runhidden; RunOnceId: "KillSpeakType"

[Code]
function CudaInSilent(): Boolean;
begin
  { Silent installs (CI smoke test) skip the 430 MB download unless /CUDALIBS is given. }
  Result := Pos('/CUDALIBS', Uppercase(GetCmdTail())) > 0;
end;

procedure CudaResult();
var
  App: String;
begin
  App := ExpandConstant('{app}');
  if not (FileExists(App + '\cudart64_12.dll') and FileExists(App + '\cublas64_12.dll') and FileExists(App + '\cublasLt64_12.dll')) then
    if not WizardSilent() then
      MsgBox(CustomMessage('CudaFailed'), mbInformation, MB_OK);
end;

function DataDir(): String;
begin
  Result := ExpandConstant('{localappdata}\SpeakType');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    { The app can also create the Run value from Settings: always remove it. }
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'SpeakType');
    if DirExists(DataDir()) then
    begin
      { Silent uninstall keeps user data unless /PURGE is passed. }
      if UninstallSilent() then
      begin
        if Pos('/PURGE', Uppercase(GetCmdTail())) > 0 then
          DelTree(DataDir(), True, True, True);
      end
      else if MsgBox(FmtMessage(CustomMessage('PurgeData'), [DataDir()]), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir(), True, True, True);
    end;
  end;
end;
