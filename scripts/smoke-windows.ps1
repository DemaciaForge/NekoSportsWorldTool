param(
    [Parameter(Mandatory = $true)]
    [string]$Archive,
    [string]$OutputDirectory = "diagnostics/windows-startup",
    [ValidateSet("full", "cli")]
    [string]$Mode = "full",
    [ValidateRange(70, 300)]
    [int]$StartupSeconds = 70
)

# Verify the packaged executable, using only a new temporary directory for its data.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$archivePath = (Resolve-Path -LiteralPath $Archive).Path
$diagnosticsPath = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $diagnosticsPath | Out-Null
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$isolatedPath = Join-Path $tempRoot ("neko-windows-startup-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $isolatedPath | Out-Null
$summaryPath = Join-Path $diagnosticsPath "summary.log"
$guiProcess = $null
$guiStdout = $null
$guiStderr = $null

function New-SmokeProcess([string]$Executable, [string]$Arguments) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $Executable
    $info.Arguments = $Arguments
    $info.WorkingDirectory = Split-Path -Parent $Executable
    $info.UseShellExecute = $false
    # Hides the console only. The GUI window must remain available for detection.
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    $info.EnvironmentVariables["RUST_BACKTRACE"] = "1"
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $info
    if (-not $process.Start()) { throw "Could not start the packaged executable." }
    return $process
}

function Invoke-SmokeCommand([string]$Executable, [string]$Arguments, [string]$Name) {
    $process = New-SmokeProcess $Executable $Arguments
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    try {
        if (-not $process.WaitForExit(30000)) {
            $process.Kill()
            $process.WaitForExit()
            throw "$Name exceeded the 30 second timeout."
        }
        $output = $stdout.GetAwaiter().GetResult()
        $errorOutput = $stderr.GetAwaiter().GetResult()
        $output | Set-Content -LiteralPath (Join-Path $diagnosticsPath "$Name.stdout.log") -Encoding utf8
        $errorOutput | Set-Content -LiteralPath (Join-Path $diagnosticsPath "$Name.stderr.log") -Encoding utf8
        "$Name exit code: $($process.ExitCode)" | Add-Content -LiteralPath $summaryPath
        if ($process.ExitCode -ne 0) { throw "$Name failed with exit code $($process.ExitCode)." }
        if ($errorOutput -match "(?i)panicked at|thread '.+' panicked") { throw "$Name emitted a Rust panic." }
        if ($Name -eq "help" -and $output -notmatch "NekoSportsWorldTool CLI") { throw "CLI help output was missing." }
        if ($Name -eq "dry-run" -and $output -notmatch "dry-run.+OBS key") { throw "CLI dry-run did not report success." }
    }
    finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        # Preserve diagnostics even when a command times out.
        $stdout.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $diagnosticsPath "$Name.stdout.log") -Encoding utf8
        $stderr.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $diagnosticsPath "$Name.stderr.log") -Encoding utf8
        $process.Dispose()
    }
}

try {
    "Mode: $Mode" | Set-Content -LiteralPath $summaryPath -Encoding utf8
    "Archive SHA256: $((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash)" | Add-Content -LiteralPath $summaryPath
    Expand-Archive -LiteralPath $archivePath -DestinationPath $isolatedPath
    $executables = @(Get-ChildItem -LiteralPath $isolatedPath -Recurse -File -Filter nekosportsworldtool.exe)
    if ($executables.Count -ne 1) { throw "Expected exactly one executable in the ZIP, found $($executables.Count)." }
    $executable = $executables[0].FullName
    $dataPath = Split-Path -Parent $executable
    if (@(Get-ChildItem -LiteralPath $dataPath -File -Filter *.json).Count -ne 0) {
        throw "The startup fixture must not contain account or configuration JSON."
    }

    if ($Mode -eq "full") {
        $guiProcess = New-SmokeProcess $executable ""
        $guiStdout = $guiProcess.StandardOutput.ReadToEndAsync()
        $guiStderr = $guiProcess.StandardError.ReadToEndAsync()
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $windowSeen = $false
        while ($timer.Elapsed.TotalSeconds -lt $StartupSeconds) {
            $guiProcess.Refresh()
            if ($guiProcess.HasExited) { throw "GUI exited during startup with code $($guiProcess.ExitCode)." }
            if ($guiProcess.MainWindowHandle -ne [IntPtr]::Zero) { $windowSeen = $true }
            if (-not $windowSeen -and $timer.Elapsed.TotalSeconds -ge 20) {
                throw "GUI did not create a window within 20 seconds; the runner may lack a usable graphical environment."
            }
            Start-Sleep -Seconds 1
        }
        $guiProcess.Refresh()
        if ($guiProcess.HasExited) { throw "GUI exited with code $($guiProcess.ExitCode)." }
        if ($guiProcess.MainWindowHandle -eq [IntPtr]::Zero) { throw "GUI window disappeared before the startup check completed." }
        "GUI created a window and remained alive for $StartupSeconds seconds." | Add-Content -LiteralPath $summaryPath
        $guiProcess.Kill()
        $guiProcess.WaitForExit()
        $guiError = $guiStderr.GetAwaiter().GetResult()
        if ($guiError -match "(?i)panicked at|thread '.+' panicked") {
            throw "GUI emitted a panic or startup error."
        }
    }
    else {
        "CLI-only mode: GUI startup was not tested." | Add-Content -LiteralPath $summaryPath
    }

    Invoke-SmokeCommand $executable "--help" "help"
    Invoke-SmokeCommand $executable "run --dry-run --dist 2 --pace 360 --lat 39.9 --lon 116.4 --seed 38" "dry-run"
    "PASS: packaged Windows executable checks completed." | Add-Content -LiteralPath $summaryPath
}
catch {
    "FAIL: $($_.Exception.Message)" | Add-Content -LiteralPath $summaryPath
    throw
}
finally {
    if ($null -ne $guiProcess) {
        if (-not $guiProcess.HasExited) { $guiProcess.Kill(); $guiProcess.WaitForExit() }
        if ($null -ne $guiStdout) {
            $guiStdout.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $diagnosticsPath "gui.stdout.log") -Encoding utf8
        }
        if ($null -ne $guiStderr) {
            $guiStderr.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $diagnosticsPath "gui.stderr.log") -Encoding utf8
        }
        $guiProcess.Dispose()
    }
    # Resolve and check the deletion target before removing the temporary fixture.
    $resolvedIsolatedPath = [IO.Path]::GetFullPath($isolatedPath)
    $tempPrefix = $tempRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedIsolatedPath.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a fixture outside the temporary directory."
    }
    if (Test-Path -LiteralPath $resolvedIsolatedPath) {
        Remove-Item -LiteralPath $resolvedIsolatedPath -Recurse -Force
    }
}
