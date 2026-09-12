param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerDirectory,

  [ValidateSet('true', 'false')]
  [string]$ReleaseValidation = 'false'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
  throw 'RUNNER_TEMP is required for the Windows installer smoke test.'
}

$signToolPath = $null
if ($ReleaseValidation -eq 'true') {
  if ([string]::IsNullOrWhiteSpace($env:OPENQUOTA_EXPECTED_WINDOWS_SIGNER_SUBJECT)) {
    throw 'OPENQUOTA_EXPECTED_WINDOWS_SIGNER_SUBJECT is required for release validation.'
  }
  $signToolCommand = Get-Command 'signtool.exe' -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -ne $signToolCommand) {
    $signToolPath = $signToolCommand.Source
  }
  if ([string]::IsNullOrWhiteSpace($signToolPath)) {
    $windowsKits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $signToolPath = Get-ChildItem -LiteralPath $windowsKits -Filter 'signtool.exe' -File -Recurse |
      Where-Object { $_.FullName -match '\\(x64|arm64)\\signtool\.exe$' } |
      Sort-Object -Property FullName -Descending |
      Select-Object -First 1 -ExpandProperty FullName
  }
  if ([string]::IsNullOrWhiteSpace($signToolPath)) {
    throw 'signtool.exe is required for Windows release validation.'
  }
}

function Assert-ReleaseSignature {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path
  )

  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "Invalid Authenticode signature on '$Path': $($signature.StatusMessage)"
  }
  if ($null -eq $signature.SignerCertificate -or
      $signature.SignerCertificate.Subject -ne $env:OPENQUOTA_EXPECTED_WINDOWS_SIGNER_SUBJECT) {
    throw "Unexpected Authenticode signer on '$Path'."
  }
  if ($null -eq $signature.TimeStamperCertificate) {
    throw "Authenticode signature on '$Path' is not timestamped."
  }

  & $signToolPath verify /pa /all /tw $Path | Out-Host
  if ($LASTEXITCODE -ne 0) {
    throw "SignTool rejected '$Path' with exit code $LASTEXITCODE."
  }
  return $signature
}

$originalLocalAppData = [Environment]::GetEnvironmentVariable('LOCALAPPDATA')
if (![string]::IsNullOrWhiteSpace($originalLocalAppData)) {
  $existingBinary = Join-Path $originalLocalAppData 'Usage01\Usage01.exe'
  if (Test-Path -LiteralPath $existingBinary -PathType Leaf) {
    throw "Refusing to disturb an existing Usage01 installation: $existingBinary"
  }
}
$uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
if (Test-Path -LiteralPath $uninstallRoot -PathType Container) {
  $existingRegistration = Get-ChildItem -LiteralPath $uninstallRoot | Where-Object {
    $_.GetValue('DisplayName') -eq 'Usage01' -or
    $_.GetValue('InstallLocation') -like '*\Usage01*' -or
    $_.GetValue('UninstallString') -like '*\Usage01\uninstall.exe*'
  } | Select-Object -First 1
  if ($null -ne $existingRegistration) {
    throw "Refusing to replace an existing Usage01 uninstall registration: $($existingRegistration.Name)"
  }
}

if (!(Test-Path -LiteralPath $InstallerDirectory -PathType Container)) {
  throw "Usage01 NSIS directory was not found: $InstallerDirectory"
}
$installers = @(Get-ChildItem -LiteralPath $InstallerDirectory -Filter '*-setup.exe' -File)
if ($installers.Count -ne 1) {
  throw "Expected exactly one Usage01 NSIS installer, found $($installers.Count)."
}
$installer = $installers[0].FullName
$installerSignature = $null
if ($ReleaseValidation -eq 'true') {
  $installerSignature = Assert-ReleaseSignature -Path $installer
}

$smokeRoot = Join-Path $env:RUNNER_TEMP "usage01-windows-$PID"
$installRoot = Join-Path $smokeRoot 'install'
$env:APPDATA = Join-Path $smokeRoot 'roaming'
$env:LOCALAPPDATA = Join-Path $smokeRoot 'local'
New-Item -ItemType Directory -Force -Path $installRoot, $env:APPDATA, $env:LOCALAPPDATA | Out-Null
$stdout = Join-Path $smokeRoot 'stdout.log'
$stderr = Join-Path $smokeRoot 'stderr.log'
$appLog = Join-Path $env:LOCALAPPDATA 'Usage01\logs\Usage01.log'

$process = $null
$uninstaller = $null
$uninstallComplete = $false
try {
  $installProcess = Start-Process -FilePath $installer -ArgumentList @('/S', "/D=$installRoot") -PassThru -Wait
  if ($installProcess.ExitCode -ne 0) {
    throw "Usage01 NSIS installer exited with code $($installProcess.ExitCode)."
  }

  $binaries = @(
    Get-ChildItem -LiteralPath $installRoot -Filter 'usage01.exe' -File -Recurse |
      Where-Object { $_.Name -notlike 'uninstall*' }
  )
  if ($binaries.Count -ne 1) {
    throw "Expected exactly one installed Usage01 binary, found $($binaries.Count)."
  }
  $binary = $binaries[0].FullName
  $uninstallers = @(Get-ChildItem -LiteralPath $installRoot -Filter 'uninstall*.exe' -File -Recurse)
  if ($uninstallers.Count -ne 1) {
    throw "Expected exactly one Usage01 uninstaller, found $($uninstallers.Count)."
  }
  $uninstaller = $uninstallers[0].FullName

  if ($ReleaseValidation -eq 'true') {
    $binarySignature = Assert-ReleaseSignature -Path $binary
    if ($null -eq $installerSignature.SignerCertificate -or
        $null -eq $binarySignature.SignerCertificate -or
        $installerSignature.SignerCertificate.Thumbprint -ne $binarySignature.SignerCertificate.Thumbprint) {
      throw 'The NSIS installer and installed Usage01 binary do not have the same Authenticode signer.'
    }
  }

  $process = Start-Process -FilePath $binary -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr

  $trayReady = $false
  $startupComplete = $false
  for ($attempt = 0; $attempt -lt 60; $attempt++) {
    if ($process.HasExited) {
      Get-Content -LiteralPath $stdout, $stderr, $appLog -ErrorAction SilentlyContinue
      throw 'Usage01 exited during the Windows tray startup smoke test.'
    }
    if (Test-Path -LiteralPath $appLog -PathType Leaf) {
      [string]$appLogContents = Get-Content -LiteralPath $appLog -Raw -ErrorAction SilentlyContinue
      $trayReady = $appLogContents.Contains('system tray integration ready')
      $startupComplete = $appLogContents.Contains('Usage01 startup completed')
      if ($trayReady -and $startupComplete) {
        break
      }
    }
    Start-Sleep -Seconds 1
  }
  if (!$trayReady -or !$startupComplete) {
    Get-Content -LiteralPath $stdout, $stderr, $appLog -ErrorAction SilentlyContinue
    throw 'Usage01 did not report a ready Windows tray before the startup deadline.'
  }

  $bytes = [System.IO.File]::ReadAllBytes($binary)
  $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
  $optionalHeader = $peOffset + 24
  $subsystem = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
  if ($subsystem -ne 2) {
    throw "Expected Windows GUI subsystem (2), found $subsystem."
  }

  Stop-Process -Id $process.Id -Force
  $process.WaitForExit(10000)
  $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList '/S' -PassThru -Wait
  if ($uninstallProcess.ExitCode -ne 0) {
    throw "Usage01 NSIS uninstaller exited with code $($uninstallProcess.ExitCode)."
  }
  for ($attempt = 0; $attempt -lt 30 -and (Test-Path -LiteralPath $binary); $attempt++) {
    Start-Sleep -Milliseconds 500
  }
  if (Test-Path -LiteralPath $binary) {
    throw 'Usage01 remained installed after the NSIS uninstall smoke test.'
  }
  $uninstallComplete = $true
} finally {
  if ($null -ne $process -and !$process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    $process.WaitForExit(10000)
  }
  if (!$uninstallComplete) {
    if ($null -eq $uninstaller) {
      $uninstaller = Get-ChildItem -LiteralPath $installRoot -Filter 'uninstall*.exe' -File -Recurse |
        Select-Object -First 1 -ExpandProperty FullName
    }
    if ($null -ne $uninstaller -and (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
      Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -ErrorAction SilentlyContinue
    }
  }
}
