param(
    [Parameter(Mandatory = $true)]
    [string]$Archive,
    [string]$OutputDirectory = "diagnostics/windows-startup",
    [ValidateSet("full", "cli")]
    [string]$Mode = "full",
    [switch]$UseSoftwareOpenGL,
    [string]$MesaArchive,
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
    if ($UseSoftwareOpenGL -and $Mode -eq "full") {
        $info.EnvironmentVariables["GALLIUM_DRIVER"] = "llvmpipe"
        $info.EnvironmentVariables["LIBGL_ALWAYS_SOFTWARE"] = "true"
        $info.EnvironmentVariables["WGL_DISABLE_ERROR_DIALOGS"] = "1"
        $info.EnvironmentVariables["MESA_SHADER_CACHE_DIR"] = Join-Path $isolatedPath "mesa-shader-cache"
    }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $info
    if (-not $process.Start()) { throw "Could not start the packaged executable." }
    return $process
}

function Install-SmokeSoftwareOpenGL([string]$DataPath) {
    # Mesa documents per-application DLL deployment and llvmpipe selection:
    # https://docs.mesa3d.org/drivers/llvmpipe.html#windows
    # https://docs.mesa3d.org/envvars.html#gallium-driver
    # This fixed asset's digest was verified against the publisher's release API:
    # https://github.com/pal1000/mesa-dist-win/releases/tag/26.2.4
    $mesaUrl = "https://github.com/pal1000/mesa-dist-win/releases/download/26.2.4/mesa3d-26.2.4-release-msvc.7z"
    $expectedHash = "351fc8c8b695878ffb3eaa044b3ead08672a48b1a045e3c3e3975811df0f6695"
    if ($MesaArchive) {
        $mesaArchivePath = (Resolve-Path -LiteralPath $MesaArchive).Path
    }
    else {
        $mesaArchivePath = Join-Path $isolatedPath "mesa3d-26.2.4-release-msvc.7z"
        Invoke-WebRequest -Uri $mesaUrl -OutFile $mesaArchivePath -TimeoutSec 180
    }
    $actualHash = (Get-FileHash -LiteralPath $mesaArchivePath -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) { throw "Mesa3D archive SHA256 did not match the pinned digest." }
    "Test renderer: Mesa3D 26.2.4 llvmpipe; archive SHA256 $actualHash" | Add-Content -LiteralPath $summaryPath
    $mesaPath = Join-Path $isolatedPath "mesa-driver"
    New-Item -ItemType Directory -Path $mesaPath | Out-Null
    # Extract only the two OpenGL DLLs. Never execute the package's deployment tools.
    tar.exe -xf $mesaArchivePath -C $mesaPath x64/opengl32.dll x64/libgallium_wgl.dll
    if ($LASTEXITCODE -ne 0) { throw "Could not extract the pinned Mesa3D test DLLs." }
    foreach ($name in @("opengl32.dll", "libgallium_wgl.dll")) {
        $source = Join-Path (Join-Path $mesaPath "x64") $name
        $destination = Join-Path $DataPath $name
        if (Test-Path -LiteralPath $destination) { throw "Refusing to overwrite a DLL from the release archive: $name" }
        Copy-Item -LiteralPath $source -Destination $destination
    }
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
        if ($UseSoftwareOpenGL) { Install-SmokeSoftwareOpenGL $dataPath }
        $guiProcess = New-SmokeProcess $executable ""
        $guiStdout = $guiProcess.StandardOutput.ReadToEndAsync()
        $guiStderr = $guiProcess.StandardError.ReadToEndAsync()
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $windowSeen = $false
        while ($timer.Elapsed.TotalSeconds -lt $StartupSeconds) {
            $guiProcess.Refresh()
            if ($guiProcess.HasExited) { throw "GUI exited during startup with code $($guiProcess.ExitCode)." }
            if (-not $windowSeen -and $guiProcess.MainWindowHandle -ne [IntPtr]::Zero) {
                $windowSeen = $true
                "GUI window created after $([Math]::Round($timer.Elapsed.TotalSeconds, 1)) seconds." | Add-Content -LiteralPath $summaryPath
                if ($UseSoftwareOpenGL) {
                    $loadedDriver = @($guiProcess.Modules | Where-Object {
                        $_.ModuleName -eq "libgallium_wgl.dll" -and $_.FileName -eq (Join-Path $dataPath "libgallium_wgl.dll")
                    })
                    if ($loadedDriver.Count -ne 1) { throw "GUI did not load the isolated Mesa3D software OpenGL driver." }
                    "Confirmed isolated libgallium_wgl.dll was loaded by the GUI process." | Add-Content -LiteralPath $summaryPath
                }
            }
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
