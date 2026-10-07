# Builds the vmangos server image with World of Skatecraft's patch (setup/vmangos/skatecraft.patch:
# the Skateboarding profession) and points a vmangos-deploy checkout's compose.yaml at it.
# setup/setup.ps1 runs this for server\; for an existing install:
#   powershell -ExecutionPolicy Bypass -File setup\build-server.ps1 -Deploy E:\path\to\server
#   docker compose -p world-of-skatecraft -f E:\path\to\server\compose.yaml up -d
# The first build compiles vmangos (a few minutes). SKATECRAFT_BUILD_JOBS sets its parallelism
# (default 4, so the PC stays usable). Mirrors setup/build-server.sh.
param(
    [string]$Deploy = ""
)
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
if (-not $Deploy) { $Deploy = Join-Path $Root 'server' }
$Patch = Join-Path $PSScriptRoot 'vmangos\skatecraft.patch'
# The vmangos commit the patch is made against.
$VmangosRev = '0e3ff01e76d4758e8a7c3108b2717cc785ed56fa'
$Jobs = if ($env:SKATECRAFT_BUILD_JOBS) { $env:SKATECRAFT_BUILD_JOBS } else { '4' }

function Step($text) { Write-Host ''; Write-Host "==> $text" -ForegroundColor Cyan }
function Stop-Build($message, $fix = '') {
    Write-Host ''
    Write-Host "SETUP STOPPED: $message" -ForegroundColor Red
    if ($fix) { Write-Host "Fix: $fix" }
    exit 1
}
function Invoke-Native($what, [scriptblock]$command) {
    & $command
    if ($LASTEXITCODE -ne 0) { Stop-Build "$what failed (see the lines above)" }
}
function Test-Native([scriptblock]$command) {
    try { & $command *> $null; return $LASTEXITCODE -eq 0 } catch { return $false }
}
# Byte-exact (Latin-1) text round trip, so no encoding or BOM is ever added or lost.
$Latin1 = [Text.Encoding]::GetEncoding(28591)
function ConvertTo-Lf($path) {
    $bytes = [IO.File]::ReadAllBytes($path)
    if ($bytes -contains 0) { return }
    $text = $Latin1.GetString($bytes)
    $fixed = $text.Replace("`r`n", "`n")
    if ($fixed -ne $text) { [IO.File]::WriteAllBytes($path, $Latin1.GetBytes($fixed)) }
}

$ComposeFile = Join-Path $Deploy 'compose.yaml'
if (-not (Test-Path $ComposeFile)) { Stop-Build "no compose.yaml in $Deploy" 'pass the server folder: -Deploy <folder>' }
if ($Jobs -notmatch '^[1-9][0-9]*$') { Stop-Build "SKATECRAFT_BUILD_JOBS must be a number, not '$Jobs'" }

# Tagged by the patch's contents, so a changed patch builds a new image.
$Hash = (Get-FileHash -Algorithm SHA256 -Path $Patch).Hash.ToLowerInvariant()
$Image = "world-of-skatecraft/vmangos-server:5875-$($Hash.Substring(0, 12))"

if (-not (Test-Native { docker image inspect $Image })) {
    Step "Building $Image (vmangos $VmangosRev with the Skatecraft patch, $Jobs jobs)"
    $Context = Join-Path ([IO.Path]::GetTempPath()) "skatecraft-build-$([guid]::NewGuid().ToString('N'))"
    try {
        New-Item -ItemType Directory -Force -Path $Context | Out-Null
        Copy-Item -Recurse (Join-Path $Deploy 'docker') (Join-Path $Context 'docker')
        New-Item -ItemType Directory -Force -Path (Join-Path $Context 'docker\patches') | Out-Null
        Copy-Item $Patch (Join-Path $Context 'docker\patches\zz-skatecraft.patch')
        # Shell scripts and the patch run in a Linux container; CRLF breaks them.
        Get-ChildItem -Recurse -File (Join-Path $Context 'docker') | ForEach-Object { ConvertTo-Lf $_.FullName }
        $Dockerfile = Join-Path $Context 'Dockerfile.skatecraft'
        $text = [IO.File]::ReadAllText((Join-Path $Context 'docker\server\Dockerfile'), [Text.Encoding]::UTF8)
        $text = $text.Replace('make -j"$(nproc)"', "make -j$Jobs")
        [IO.File]::WriteAllText($Dockerfile, $text, (New-Object Text.UTF8Encoding($false)))
        Invoke-Native 'the server build' {
            docker build -t $Image -f $Dockerfile `
                --build-arg "VMANGOS_REVISION=$VmangosRev" `
                --build-arg VMANGOS_CLIENT_VERSION=5875 `
                --build-arg VMANGOS_FAIL_ON_PATCH_ERROR=1 `
                $Context
        }
    }
    finally {
        Remove-Item -Recurse -Force $Context -ErrorAction SilentlyContinue
    }
}

# Targeted replace on the raw bytes: line endings and everything else in the file stay as they are.
$compose = $Latin1.GetString([IO.File]::ReadAllBytes($ComposeFile))
$pattern = 'image: (ghcr\.io/mserajnik/vmangos-server:5875|world-of-skatecraft/vmangos-server:\S+)'
if ($compose -notmatch $pattern) { Stop-Build "no vmangos-server image: line found in $ComposeFile" }
$updated = [regex]::Replace($compose, $pattern, "image: $Image")
if ($updated -ne $compose) { [IO.File]::WriteAllBytes($ComposeFile, $Latin1.GetBytes($updated)) }
Write-Host "$ComposeFile now runs $Image"
exit 0
