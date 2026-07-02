#include "CodeDependencies.iss"

#define MyAppName "Azookey"
#define MyAppVersion "0.1.0-alpha.1"
#define MyAppPublisher "fkunn1326"
#define MyAppURL "https://github.com/fkunn1326/azooKey-Windows/"

[Setup]
AppId={{80B746D4-D74D-4345-8F81-47E06BCAB515}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={userappdata}\{#MyAppName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
OutputDir=../build
OutputBaseFilename=azookey-setup
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
UsedUserAreasWarning=no
CloseApplications=yes
CloseApplicationsFilter=launcher.exe,azookey-server.exe,ui.exe,azookey_settings.exe

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Files]
; Task scheduler XML — extracted to temp dir only, not installed
Source: "./Azookey Startup.xml"; Flags: dontcopy noencryption
; Register 64-bit IME DLL (also handles DllUnregisterServer on uninstall)
Source: "../build/azookey_windows.dll"; DestDir: "{app}"; DestName: "azookey.dll"; Flags: ignoreversion regserver 64bit
; Register 32-bit IME DLL
Source: "../build/x86/azookey_windows.dll"; DestDir: "{app}"; DestName: "azookey32.dll"; Flags: ignoreversion regserver 32bit
; All other build artifacts (exes, dictionaries, etc.) — excludes the raw DLLs already handled above
Source: "../build/*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "azookey_windows.dll,x86\azookey_windows.dll"

[Run]
; Grant AppContainer (sandbox) read/execute on the IME DLLs — required for TSF to load the DLL
Filename: "icacls"; \
  Parameters: """{app}\azookey.dll"" /grant ""*S-1-15-2-1:(RX)"""; \
  Flags: runhidden runascurrentuser
Filename: "icacls"; \
  Parameters: """{app}\azookey32.dll"" /grant ""*S-1-15-2-1:(RX)"""; \
  Flags: runhidden runascurrentuser

[UninstallRun]
; Remove the startup scheduled task
Filename: "schtasks"; \
  Parameters: "/Delete /TN ""Azookey Startup"" /F"; \
  Flags: runhidden runascurrentuser; \
  RunOnceId: "DeleteAzookeyTask"

[UninstallDelete]
; Remove files created at runtime (not installed by the [Files] section)
Type: files; Name: "{app}\launch.vbs"

[Code]
function InitializeSetup: Boolean;
begin
  Dependency_AddVC2015To2022x64;
  Dependency_AddVC2015To2022x86;
  Result := True;
end;

function UninstallNeedRestart(): Boolean;
begin
  Result := False;
end;

procedure KillProcesses();
var
  Dummy: Integer;
begin
  ShellExec('', 'taskkill', '/F /IM launcher.exe',       '', SW_HIDE, ewWaitUntilTerminated, Dummy);
  ShellExec('', 'taskkill', '/F /IM azookey-server.exe', '', SW_HIDE, ewWaitUntilTerminated, Dummy);
  ShellExec('', 'taskkill', '/F /IM ui.exe',             '', SW_HIDE, ewWaitUntilTerminated, Dummy);
  ShellExec('', 'taskkill', '/F /IM azookey_settings.exe', '', SW_HIDE, ewWaitUntilTerminated, Dummy);
end;

procedure CreateVbsFile();
var
  VbsFile: string;
  VbsContent: AnsiString;
begin
  VbsFile := ExpandConstant('{app}\launch.vbs');
  VbsContent :=
    'Set objShell = CreateObject("WScript.Shell")' + #13#10 +
    'objShell.Run "' + ExpandConstant('{app}\launcher.exe') + '", 0, False' + #13#10;
  SaveStringToFile(VbsFile, VbsContent, False);
end;

procedure UpdateTaskXml();
var
  TaskXmlPath: string;
  TaskXmlContentAnsi: AnsiString;
  TaskXmlContent: String;
  Dummy: Integer;
begin
  ExtractTemporaryFile('Azookey Startup.xml');
  TaskXmlPath := ExpandConstant('{tmp}\Azookey Startup.xml');

  LoadStringFromFile(TaskXmlPath, TaskXmlContentAnsi);
  TaskXmlContent := String(TaskXmlContentAnsi);
  StringChange(TaskXmlContent, 'PATH_TO_VBS', ExpandConstant('{app}\launch.vbs'));
  TaskXmlContentAnsi := AnsiString(TaskXmlContent);
  SaveStringToFile(TaskXmlPath, TaskXmlContentAnsi, False);

  ShellExec('', 'schtasks', '/Create /F /TN "Azookey Startup" /XML "' + TaskXmlPath + '"', '', SW_HIDE, ewWaitUntilTerminated, Dummy);
  ShellExec('', 'schtasks', '/Run /TN "Azookey Startup"', '', SW_HIDE, ewWaitUntilTerminated, Dummy);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    KillProcesses();
  if CurStep = ssPostInstall then
  begin
    CreateVbsFile();
    UpdateTaskXml();
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpFinished then
    WizardForm.RunList.Visible := False;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    KillProcesses();
end;
