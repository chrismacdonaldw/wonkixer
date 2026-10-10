<#
Builds the signed wonkixer NSIS installer, its updater signature and latest.json.

  .\scripts\build-release.ps1 -Notes "Summary of this release"
  .\scripts\build-release.ps1 -Notes "Summary of this release" -Publish

-Publish creates the GitHub release vX.Y.Z from the tag at HEAD and uploads the assets.
-AllowDirty builds from an uncommitted tree; it cannot be combined with -Publish.
#>
[CmdletBinding()]
param(
    [string]$Notes,
    [switch]$Publish,
    [switch]$AllowDirty,
    [string]$SigningKey = (Join-Path $HOME ".tauri\wonkixer.key"),
    [string]$SigningPasswordFile = (Join-Path $HOME ".tauri\wonkixer.key.password"),
    [string]$Repository = "chrismacdonaldw/wonkixer"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Checked {
    param([Parameter(Mandatory)][string]$Command, [string[]]$Arguments = @())
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE."
    }
}

function Test-Command {
    param([Parameter(Mandatory)][string]$Command, [string[]]$Arguments = @())
    $ErrorActionPreference = "Continue"
    & $Command @Arguments *> $null
    return $LASTEXITCODE -eq 0
}

function Get-GitOutput {
    param([Parameter(Mandatory)][string[]]$Arguments)
    $output = & git @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed with exit code $LASTEXITCODE."
    }
    return $output
}

function Restore-Environment {
    param([hashtable]$Saved)
    foreach ($name in $Saved.Keys) {
        if ($null -eq $Saved[$name]) {
            Remove-Item "Env:$name" -ErrorAction SilentlyContinue
        } else {
            Set-Item "Env:$name" $Saved[$name]
        }
    }
}

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if ($Publish -and $AllowDirty) {
        throw "-Publish requires a committed tree; drop -AllowDirty."
    }
    $changes = @(Get-GitOutput @("status", "--porcelain"))
    if ($changes.Count -gt 0) {
        if (-not $AllowDirty) {
            throw "The working tree has uncommitted changes. Commit them or pass -AllowDirty for a local test build."
        }
        Write-Warning "Building from a dirty tree; these assets are for local testing only."
    }

    $config = Get-Content -LiteralPath (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
    $metadataJson = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $root "Cargo.toml")
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE."
    }
    $metadata = $metadataJson | ConvertFrom-Json
    $appPackage = @($metadata.packages | Where-Object { $_.name -eq "wonkixer" })
    if ($appPackage.Count -ne 1) {
        throw "Could not find the wonkixer package in the Cargo workspace."
    }

    $version = (Get-Content -LiteralPath (Join-Path $root "package.json") -Raw | ConvertFrom-Json).version
    $versions = [ordered]@{
        "package.json" = $version
        "Cargo.toml (workspace)" = $appPackage[0].version
        "tauri.conf.json" = if ($config.PSObject.Properties.Name -contains "version") { $config.version } else { $appPackage[0].version }
    }
    $mismatched = @($versions.Keys | Where-Object { $versions[$_] -ne $version })
    if ($mismatched.Count -gt 0) {
        $listing = ($versions.Keys | ForEach-Object { "$_ = $($versions[$_])" }) -join ", "
        throw "Versions disagree: $listing."
    }
    if ($version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
        throw "Version $version is not semantic versioning."
    }
    $tag = "v$version"
    if (-not $Notes) {
        $Notes = "wonkixer $tag"
    }

    if (-not (Test-Path -LiteralPath $SigningKey)) {
        throw "Updater signing key not found at $SigningKey. Never generate a replacement once a signed release has shipped; installed copies only trust the original key."
    }
    $publicKeyPath = "$SigningKey.pub"
    if (-not (Test-Path -LiteralPath $publicKeyPath)) {
        throw "Updater public key not found at $publicKeyPath."
    }
    if ((Get-Content -LiteralPath $publicKeyPath -Raw).Trim() -ne $config.plugins.updater.pubkey) {
        throw "The signing key does not match the updater public key in tauri.conf.json."
    }

    if ($Publish) {
        if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
            throw "The GitHub CLI (gh) is required for -Publish."
        }
        if (-not (Test-Command gh @("auth", "status"))) {
            throw "gh is not authenticated. Run gh auth login."
        }
        if (-not (Test-Command git @("rev-parse", "-q", "--verify", "refs/tags/$tag"))) {
            throw "Tag $tag does not exist. Create and push it first: git tag -a $tag -m `"wonkixer $tag`"; git push origin $tag"
        }
        $head = Get-GitOutput @("rev-parse", "HEAD")
        $tagged = Get-GitOutput @("rev-parse", "$tag^{commit}")
        if ($head -ne $tagged) {
            throw "Tag $tag points at $tagged but HEAD is $head. Check out the tagged commit before publishing."
        }
        if (Test-Command gh @("release", "view", $tag, "--repo", $Repository)) {
            throw "Release $tag already exists on $Repository. Ship corrections under a new version."
        }
    }

    Invoke-Checked npm @("ci")
    Invoke-Checked npm @("run", "check")

    $saved = @{
        TAURI_SIGNING_PRIVATE_KEY = $env:TAURI_SIGNING_PRIVATE_KEY
        TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
        RUSTFLAGS = $env:RUSTFLAGS
    }
    $signingPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
    if (-not $signingPassword -and (Test-Path -LiteralPath $SigningPasswordFile)) {
        $securePassword = Get-Content -LiteralPath $SigningPasswordFile -Raw | ConvertTo-SecureString
        $signingPassword = (New-Object Management.Automation.PSCredential("wonkixer", $securePassword)).GetNetworkCredential().Password
    }
    $env:TAURI_SIGNING_PRIVATE_KEY = $SigningKey
    if ($signingPassword) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $signingPassword
    }
    $remapFlag = "--remap-path-prefix=$HOME=~"
    $env:RUSTFLAGS = if ($saved.RUSTFLAGS) { "$($saved.RUSTFLAGS) $remapFlag" } else { $remapFlag }
    try {
        Invoke-Checked npm @("run", "tauri", "build")
    } finally {
        Restore-Environment $saved
        $signingPassword = $null
    }

    $releaseRoot = Join-Path $metadata.target_directory "release"
    $setupName = "$($config.productName)_${version}_x64-setup.exe"
    $setupPath = Join-Path $releaseRoot "bundle\nsis\$setupName"
    $signaturePath = "$setupPath.sig"
    foreach ($path in @($setupPath, $signaturePath)) {
        if (-not (Test-Path -LiteralPath $path)) {
            throw "Expected release artifact not found: $path"
        }
    }

    $assetDirectory = Join-Path $releaseRoot "release-assets"
    if (Test-Path -LiteralPath $assetDirectory) {
        Remove-Item -LiteralPath $assetDirectory -Recurse -Force
    }
    New-Item -ItemType Directory -Path $assetDirectory | Out-Null
    Copy-Item -LiteralPath $setupPath, $signaturePath -Destination $assetDirectory

    $latest = [ordered]@{
        version = $version
        notes = $Notes
        pub_date = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
        platforms = [ordered]@{
            "windows-x86_64" = [ordered]@{
                signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()
                url = "https://github.com/$Repository/releases/download/$tag/$setupName"
            }
        }
    }
    $utf8WithoutBom = New-Object Text.UTF8Encoding($false)
    $latestPath = Join-Path $assetDirectory "latest.json"
    [IO.File]::WriteAllText($latestPath, ($latest | ConvertTo-Json -Depth 5), $utf8WithoutBom)

    $assets = @(
        (Join-Path $assetDirectory $setupName),
        (Join-Path $assetDirectory "$setupName.sig"),
        $latestPath
    )
    Write-Host "Prepared wonkixer $version release assets in ${assetDirectory}:"
    foreach ($asset in $assets) {
        $file = Get-Item -LiteralPath $asset
        $hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Host ("  {0,-36} {1,12:N0} bytes  sha256 {2}" -f $file.Name, $file.Length, $hash)
    }

    $notesFile = Join-Path ([IO.Path]::GetTempPath()) "wonkixer-$tag-notes.md"
    $releaseArguments = @("release", "create", $tag) + $assets + @("--repo", $Repository, "--verify-tag", "--title", "wonkixer $tag", "--notes-file", $notesFile)
    if (-not $Publish) {
        $display = ($releaseArguments | ForEach-Object { if ($_ -match '\s') { "`"$_`"" } else { $_ } }) -join ' '
        Write-Host "Not published. With -Publish this runs: gh $display"
        return
    }
    [IO.File]::WriteAllText($notesFile, $Notes, $utf8WithoutBom)
    try {
        Invoke-Checked gh $releaseArguments
    } finally {
        Remove-Item -LiteralPath $notesFile -ErrorAction SilentlyContinue
    }
    Write-Host "Published https://github.com/$Repository/releases/tag/$tag"
} finally {
    Pop-Location
}
