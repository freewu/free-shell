# End-to-end run of the real desktop app:
#   1. start examples/fake_ssh_server.rs on a loopback port
#   2. seed a saved session in the app data dir (the user's own files are backed up)
#   3. start free-shell.exe with the WebView2 debugging port open
#   4. drive the UI over CDP (scripts/ui_e2e.mjs)
param(
  [int]$SshPort = 2222,
  [int]$CdpPort = 9222
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$exe = Join-Path $root 'src-tauri\target\debug\free-shell.exe'
$server = Join-Path $root 'src-tauri\target\debug\examples\fake_ssh_server.exe'
$appData = Join-Path $env:APPDATA 'com.freeshell.app'
$serverLog = Join-Path $env:TEMP 'fake_ssh_server.log'
$backup = Join-Path $env:TEMP ("freeshell-e2e-backup-" + [guid]::NewGuid())
$stateFiles = @('sessions.json', 'known_hosts.json')

foreach ($p in @($exe, $server)) {
  if (-not (Test-Path $p)) { throw "missing binary: $p (run 'just build-debug')" }
}

Get-Process free-shell, fake_ssh_server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 400

$app = $null
$srv = $null
$code = 1

try {
  # --- seed a session so the UI has something to click --------------------
  New-Item -ItemType Directory -Force -Path $appData, $backup | Out-Null
  foreach ($name in $stateFiles) {
    $file = Join-Path $appData $name
    if (Test-Path $file) { Copy-Item $file (Join-Path $backup $name) -Force }
  }

  $sessions = @(
    [ordered]@{
      id          = 'e2e-local'
      name        = 'e2e-local'
      host        = '127.0.0.1'
      port        = $SshPort
      username    = 'tester'
      group       = 'e2e'
      color       = '#38bdf8'
      description = 'fake ssh server used by scripts/ui_e2e.ps1'
      auth        = [ordered]@{ type = 'password' }
      password    = 's3cret'
      tags        = @('e2e')
    }
  )
  # UTF-8 without a BOM: serde_json rejects a leading BOM.
  # ConvertTo-Json unwraps single element arrays, so wrap explicitly.
  $json = $sessions | ConvertTo-Json -Depth 5
  if (-not $json.TrimStart().StartsWith('[')) { $json = "[$json]" }
  [System.IO.File]::WriteAllText(
    (Join-Path $appData 'sessions.json'),
    $json,
    (New-Object System.Text.UTF8Encoding($false))
  )

  # --- fake ssh server ----------------------------------------------------
  Remove-Item $serverLog, "$serverLog.err" -ErrorAction SilentlyContinue
  $srv = Start-Process -FilePath $server -ArgumentList '--port', "$SshPort" `
    -RedirectStandardOutput $serverLog -RedirectStandardError "$serverLog.err" `
    -PassThru -WindowStyle Hidden

  $ready = $false
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 250
    if ((Test-Path $serverLog) -and (Select-String -Path $serverLog -Pattern 'LISTENING' -Quiet)) {
      $ready = $true; break
    }
  }
  if (-not $ready) {
    Get-Content $serverLog, "$serverLog.err" -ErrorAction SilentlyContinue
    throw "fake ssh server did not start on port $SshPort"
  }
  Write-Output "fake ssh server listening on 127.0.0.1:$SshPort (pid $($srv.Id))"

  # --- the real app -------------------------------------------------------
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort"
  $app = Start-Process -FilePath $exe -PassThru

  $up = $false
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    try {
      $null = Invoke-WebRequest -Uri "http://127.0.0.1:$CdpPort/json/list" -UseBasicParsing -TimeoutSec 2
      $up = $true; break
    } catch { }
  }
  if (-not $up) { throw "app did not expose the debugging port" }
  $env:CDP_PORT = "$CdpPort"
  $env:E2E_SESSION_ID = 'e2e-local'

  Push-Location $root
  try { node (Join-Path $root 'scripts\ui_e2e.mjs'); $code = $LASTEXITCODE } finally { Pop-Location }
} finally {
  if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
  if ($srv -and -not $srv.HasExited) { Stop-Process -Id $srv.Id -Force }

  # --- put the user's own configuration back ------------------------------
  foreach ($name in $stateFiles) {
    $file = Join-Path $appData $name
    $saved = Join-Path $backup $name
    Remove-Item $file -ErrorAction SilentlyContinue
    if (Test-Path $saved) { Copy-Item $saved $file -Force }
  }
  Remove-Item -Recurse -Force $backup -ErrorAction SilentlyContinue
}

exit $code
