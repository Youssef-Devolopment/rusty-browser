; Rust Browser — installer script (Inno Setup)
; Build with: ISCC.exe installer.iss   (produces dist\RustBrowserSetup.exe)

#define AppName "Rust Browser"
#define AppVersion "0.1.0"
#define AppExe "RustBrowser.exe"
#define AppExeSrc "target\release\browser-rs.exe"

[Setup]
AppId={{7C9E4B2A-5D31-4F8E-9A66-2B8E1D3F7A41}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppName}
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; User-level install: no admin/UAC needed on the target machine.
PrivilegesRequired=lowest
OutputBaseFilename=RustBrowserSetup
OutputDir=dist
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
SetupIconFile=dist\rust_browser.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
Source: "{#AppExeSrc}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[Code]
// The browser renders pages with Microsoft's WebView2 runtime.
// Most Windows 10/11 PCs already have it; warn if this one doesn't.
function InitializeSetup(): Boolean;
var
  ErrorCode: Integer;
begin
  Result := True;
  if not (RegKeyExists(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')
     or RegKeyExists(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')
     or RegKeyExists(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')) then
  begin
    if MsgBox('This PC is missing the Microsoft Edge WebView2 runtime, which Rust Browser needs to display web pages.'#13#10#13#10
      + 'Open the Microsoft download page now?'#13#10
      + '(Download the "Evergreen Bootstrapper", run it, then start Rust Browser.)',
      mbConfirmation, MB_YESNO) = IDYES then
    begin
      ShellExec('open', 'https://developer.microsoft.com/en-us/microsoft-edge/webview2/', '', '', SW_SHOWNORMAL, False, ErrorCode);
    end;
  end;
end;
