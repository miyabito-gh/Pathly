param(
    [Parameter(Position = 0)]
    [ValidateSet('dev', 'build')]
    [string]$Mode = 'dev'
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = $PSScriptRoot

if (-not (Test-Path (Join-Path $ProjectRoot 'node_modules'))) {
    throw 'Dependencies are missing. Run npm install first.'
}

Push-Location $ProjectRoot
try {
    if ($Mode -eq 'dev') {
        & npm.cmd run tauri -- dev
        exit $LASTEXITCODE
    }

    $Executable = Join-Path $ProjectRoot 'src-tauri\target\release\Pathly.exe'
    & npm.cmd run tauri -- build --no-bundle
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    if (-not (Test-Path $Executable)) {
        throw "Built executable was not found: $Executable"
    }

    & $Executable
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
