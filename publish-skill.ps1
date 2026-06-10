<#
.SYNOPSIS
Publishes one Skill Hub skill version to a full MinIO-backed marketplace.

.DESCRIPTION
This script packages a local skill directory, calculates SHA-256, uploads the
version files, updates the per-skill manifest, reads categories from an external
categories.v1.json file, rebuilds category/search indexes, and uploads
catalog.v1.json last. If skill.json is missing, publish metadata is generated
from SKILL.md, README.md, and the directory name.

Requires MinIO Client:
  https://min.io/docs/minio/windows/reference/minio-mc.html

Before running, either configure an alias:
  mc alias set skillhub http://127.0.0.1:9000 minioadmin minioadmin

Or pass -Endpoint, -AccessKey, and -SecretKey to this script.

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market

.EXAMPLE
Publish a skill directory without writing skill.json by hand:

.\publish-skill.ps1 `
  -SkillDir .\my-skill `
  -Namespace official `
  -Version 1.0.0 `
  -Categories frontend `
  -CreateBucket

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Endpoint http://127.0.0.1:9000 `
  -AccessKey minioadmin `
  -SecretKey minioadmin `
  -CreateBucket

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market `
  -CategoriesPath .\categories.v1.json
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$SkillDir,

    [string]$Namespace,

    [string]$Version,

    [string]$SkillId,

    [string]$SkillName,

    [string]$SkillSummary,

    [Alias("Categories")]
    [string[]]$SkillCategories,

    [string]$Alias = "skillhub",

    [string]$Bucket = "skill-market",

    [string]$Endpoint,

    [string]$AccessKey,

    [string]$SecretKey,

    [string]$Region,

    [string]$ChangelogPath,

    [string]$SignaturePath,

    # Categories source JSON. Defaults to categories.v1.json next to this script.
    [string]$CategoriesPath,

    [switch]$CreateBucket,

    [switch]$AllowOverwrite,

    [switch]$DryRun,

    [switch]$KeepTemp
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$PackageFileName = "package.zip"
$HashFileName = "package.sha256"
$VersionSkillFileName = "skill.json"
$SignatureFileName = "signature.minisig"
$ChangelogFileName = "changelog.md"
$DefaultCategoriesFileName = "categories.v1.json"

function Write-Step {
    param([string]$Message)
    Write-Host "[skillhub] $Message"
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList @($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-Utf8Text {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList @($false, $true)
    return [System.IO.File]::ReadAllText($Path, $encoding)
}

function Read-JsonFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $raw = Read-Utf8Text -Path $Path
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw "JSON file is empty: $Path"
    }

    return ($raw | ConvertFrom-Json)
}

function Save-Json {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [object]$Object
    )

    $json = $Object | ConvertTo-Json -Depth 64
    Write-Utf8NoBom -Path $Path -Text ($json + [Environment]::NewLine)
}

function Normalize-Array {
    param([object]$Value)

    if ($null -eq $Value) {
        return @()
    }

    if ($Value -is [string]) {
        if ([string]::IsNullOrWhiteSpace($Value)) {
            return @()
        }

        return @($Value)
    }

    if ($Value -is [System.Array]) {
        return @($Value)
    }

    return @($Value)
}

function Set-JsonProperty {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [object]$Value
    )

    if ($Object -is [System.Collections.IDictionary]) {
        $Object[$Name] = $Value
        return
    }

    if ($Object.PSObject.Properties.Name -contains $Name) {
        $Object.$Name = $Value
    }
    else {
        $Object | Add-Member -NotePropertyName $Name -NotePropertyValue $Value
    }
}

function Get-JsonPropertyValue {
    param(
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string[]]$Names,

        [object]$Default = $null
    )

    if ($null -eq $Object) {
        return $Default
    }

    if ($Object -is [System.Collections.IDictionary]) {
        foreach ($name in $Names) {
            if ($Object.Contains($name)) {
                $value = $Object[$name]
                if ($null -ne $value) {
                    return $value
                }
            }
        }

        return $Default
    }

    $propertyNames = @($Object.PSObject.Properties.Name)
    foreach ($name in $Names) {
        if ($propertyNames -contains $name) {
            $value = $Object.PSObject.Properties[$name].Value
            if ($null -ne $value) {
                return $value
            }
        }
    }

    return $Default
}

function Join-ObjectPath {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Parts
    )

    return (($Parts | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object { $_.Trim("/") }) -join "/")
}

function Assert-SafeObjectSegment {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    if ($Value -notmatch "^[A-Za-z0-9][A-Za-z0-9._-]*$") {
        throw "$Name '$Value' is not a safe object path segment. Use letters, numbers, dot, underscore, or dash."
    }
}

function Copy-PackagePayload {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceDir,

        [Parameter(Mandatory = $true)]
        [string]$DestinationDir,

        [string]$SignatureFileName
    )

    New-Item -ItemType Directory -Path $DestinationDir -Force | Out-Null
    $sourceRoot = (Resolve-Path -LiteralPath $SourceDir).Path

    Get-ChildItem -LiteralPath $sourceRoot -Recurse -Force | Where-Object {
        -not $_.PSIsContainer -and
        $_.Extension -ne ".json" -and
        $_.Name -ne ".DS_Store" -and
        $_.Name -ne $SignatureFileName
    } | ForEach-Object {
        $relative = $_.FullName.Substring($sourceRoot.Length) -replace "^[\\/]+", ""
        $target = Join-Path $DestinationDir $relative
        $parent = Split-Path -Parent $target
        if (-not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        Copy-Item -LiteralPath $_.FullName -Destination $target -Force
    }
}

function Invoke-Mc {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$McArgs
    )

    if ($DryRun) {
        Write-Host "DRY RUN: mc $($McArgs -join ' ')"
        return
    }

    & mc @McArgs
    if ($LASTEXITCODE -ne 0) {
        throw "mc failed: mc $($McArgs -join ' ')"
    }
}

function Test-RemoteObject {
    param([Parameter(Mandatory = $true)][string]$ObjectPath)

    if ($DryRun) {
        return $false
    }

    & mc stat "$Alias/$Bucket/$ObjectPath" | Out-Null
    return ($LASTEXITCODE -eq 0)
}

function Get-RemoteJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ObjectPath,

        [Parameter(Mandatory = $true)]
        [object]$DefaultObject,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $localName = (($ObjectPath -replace "[\\/]", "_") -replace "[^A-Za-z0-9._-]", "_")
    $localPath = Join-Path $WorkDir $localName

    if (-not $DryRun) {
        & mc cp "$Alias/$Bucket/$ObjectPath" $localPath | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $localPath)) {
            $raw = Read-Utf8Text -Path $localPath
            if (-not [string]::IsNullOrWhiteSpace($raw)) {
                return ($raw | ConvertFrom-Json)
            }
        }
    }

    return (($DefaultObject | ConvertTo-Json -Depth 64) | ConvertFrom-Json)
}

function Copy-JsonObject {
    param([Parameter(Mandatory = $true)][object]$Object)
    return (($Object | ConvertTo-Json -Depth 64) | ConvertFrom-Json)
}

function Find-DefaultChangelog {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SkillDir
    )

    $candidates = @(
        (Join-Path $SkillDir "changelog.md"),
        (Join-Path $SkillDir "CHANGELOG.md")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return $null
}

function Find-DefaultSignature {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SkillDir
    )

    $candidate = Join-Path $SkillDir $SignatureFileName
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }

    return $null
}

function Convert-ToSafeObjectSegment {
    param(
        [string]$Value,
        [string]$Fallback = "skill"
    )

    $text = $Value
    if ([string]::IsNullOrWhiteSpace($text)) {
        $text = $Fallback
    }

    $slug = $text.ToLowerInvariant()
    $slug = $slug -replace "[^a-z0-9._-]+", "-"
    $slug = $slug -replace "^[._-]+", ""
    $slug = $slug -replace "[._-]+$", ""

    if ([string]::IsNullOrWhiteSpace($slug)) {
        $slug = $Fallback
    }

    if ($slug -notmatch "^[a-z0-9]") {
        $slug = "skill-$slug"
    }

    return $slug
}

function Get-MarkdownTitle {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    $content = Read-Utf8Text -Path $Path
    foreach ($line in $content -split "`r?`n") {
        $trimmed = $line.Trim()
        if (-not $trimmed.StartsWith("#")) {
            continue
        }

        $title = ($trimmed -replace "^#+", "").Trim()
        if (-not [string]::IsNullOrWhiteSpace($title)) {
            return $title
        }
    }

    return $null
}

function Get-MarkdownSummary {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    $content = Read-Utf8Text -Path $Path
    foreach ($line in $content -split "`r?`n") {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or
            $trimmed.StartsWith("#") -or
            $trimmed.StartsWith("```")) {
            continue
        }

        if ($trimmed.Length -gt 240) {
            return $trimmed.Substring(0, 240)
        }

        return $trimmed
    }

    return $null
}

function New-CatalogDefault {
    return [ordered]@{
        schema = "skillhub.catalog.v1"
        generatedAt = $script:Now
        categories = @()
        skills = @()
    }
}

function New-ManifestDefault {
    param(
        [string]$Namespace,
        [string]$SkillId,
        [string]$Name,
        [string]$Summary,
        [object[]]$Categories,
        [object[]]$Tags,
        [object[]]$Levels
    )

    return [ordered]@{
        schema = "skillhub.skill-manifest.v1"
        namespace = $Namespace
        id = $SkillId
        name = $Name
        summary = $Summary
        categories = @($Categories)
        tags = @($Tags)
        levels = @($Levels)
        latestVersion = $script:Version
        versions = @()
        updatedAt = $script:Now
    }
}

function New-SkillEntry {
    param(
        [string]$Namespace,
        [string]$SkillId,
        [string]$Name,
        [string]$Summary,
        [string]$Version,
        [object[]]$Categories,
        [object[]]$Tags,
        [object[]]$Levels,
        [string]$ManifestPath
    )

    return [ordered]@{
        namespace = $Namespace
        id = $SkillId
        name = $Name
        summary = $Summary
        latestVersion = $Version
        categories = @($Categories)
        tags = @($Tags)
        levels = @($Levels)
        manifestPath = $ManifestPath
        updatedAt = $script:Now
    }
}

function Convert-ToCanonicalSkillEntry {
    param([Parameter(Mandatory = $true)][object]$Entry)

    $namespace = Get-JsonPropertyValue -Object $Entry -Names @("namespace")
    $skillId = Get-JsonPropertyValue -Object $Entry -Names @("id")
    $latestVersion = Get-JsonPropertyValue -Object $Entry -Names @("latestVersion", "latest_version")
    $manifestPath = Get-JsonPropertyValue -Object $Entry -Names @("manifestPath", "manifest_path")

    if ([string]::IsNullOrWhiteSpace($namespace) -or
        [string]::IsNullOrWhiteSpace($skillId) -or
        [string]::IsNullOrWhiteSpace($latestVersion) -or
        [string]::IsNullOrWhiteSpace($manifestPath)) {
        throw "Remote catalog contains a skill entry missing namespace, id, latestVersion, or manifestPath."
    }

    return [ordered]@{
        namespace = $namespace
        id = $skillId
        name = Get-JsonPropertyValue -Object $Entry -Names @("name") -Default $skillId
        summary = Get-JsonPropertyValue -Object $Entry -Names @("summary") -Default ""
        latestVersion = $latestVersion
        categories = @(Normalize-Array (Get-JsonPropertyValue -Object $Entry -Names @("categories") -Default @()))
        tags = @(Normalize-Array (Get-JsonPropertyValue -Object $Entry -Names @("tags") -Default @()))
        levels = @(Normalize-Array (Get-JsonPropertyValue -Object $Entry -Names @("levels") -Default @("personal", "project")))
        manifestPath = $manifestPath
        updatedAt = Get-JsonPropertyValue -Object $Entry -Names @("updatedAt", "updated_at") -Default $script:Now
    }
}

function Convert-ToCanonicalVersionEntry {
    param([Parameter(Mandatory = $true)][object]$Entry)

    $version = Get-JsonPropertyValue -Object $Entry -Names @("version")
    $skillPath = Get-JsonPropertyValue -Object $Entry -Names @("skillPath", "skill_path")
    $packagePath = Get-JsonPropertyValue -Object $Entry -Names @("packagePath", "package_path")
    $sha256Path = Get-JsonPropertyValue -Object $Entry -Names @("sha256Path", "sha256_path")

    if ([string]::IsNullOrWhiteSpace($version) -or
        [string]::IsNullOrWhiteSpace($skillPath) -or
        [string]::IsNullOrWhiteSpace($packagePath) -or
        [string]::IsNullOrWhiteSpace($sha256Path)) {
        throw "Remote manifest contains a version entry missing version, skillPath, packagePath, or sha256Path."
    }

    $canonical = [ordered]@{
        version = $version
        skillPath = $skillPath
        packagePath = $packagePath
        sha256Path = $sha256Path
    }

    $changelogPath = Get-JsonPropertyValue -Object $Entry -Names @("changelogPath", "changelog_path")
    if (-not [string]::IsNullOrWhiteSpace($changelogPath)) {
        $canonical["changelogPath"] = $changelogPath
    }

    $signaturePath = Get-JsonPropertyValue -Object $Entry -Names @("signaturePath", "signature_path")
    if (-not [string]::IsNullOrWhiteSpace($signaturePath)) {
        $canonical["signaturePath"] = $signaturePath
    }

    $createdAt = Get-JsonPropertyValue -Object $Entry -Names @("createdAt", "created_at")
    if (-not [string]::IsNullOrWhiteSpace($createdAt)) {
        $canonical["createdAt"] = $createdAt
    }

    $package = Get-JsonPropertyValue -Object $Entry -Names @("package")
    if ($null -ne $package) {
        $canonical["package"] = [ordered]@{
            file = Get-JsonPropertyValue -Object $package -Names @("file") -Default $PackageFileName
            sha256 = Get-JsonPropertyValue -Object $package -Names @("sha256") -Default ""
            size = Get-JsonPropertyValue -Object $package -Names @("size") -Default 0
        }
    }

    return $canonical
}

function Convert-ToCanonicalCategoriesDoc {
    param([Parameter(Mandatory = $true)][object]$CategoriesDoc)

    $items = @(Normalize-Array (Get-JsonPropertyValue -Object $CategoriesDoc -Names @("items") -Default @()) | ForEach-Object {
        [ordered]@{
            id = $_.id
            name = $_.name
            order = $_.order
        }
    })

    return [ordered]@{
        schema = "skillhub.categories.v1"
        generatedAt = $script:Now
        items = @($items)
    }
}

function Build-CategoryIndexes {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Catalog,

        [Parameter(Mandatory = $true)]
        [object]$CategoriesDoc,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $uploaded = @()
    $skills = Normalize-Array (Get-JsonPropertyValue -Object $Catalog -Names @("skills") -Default @())
    $categoryIds = Normalize-Array (Get-JsonPropertyValue -Object $CategoriesDoc -Names @("items") -Default @()) | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique

    foreach ($categoryId in $categoryIds) {
        Assert-SafeObjectSegment -Name "category id" -Value $categoryId

        $items = @($skills | Where-Object {
            (Normalize-Array (Get-JsonPropertyValue -Object $_ -Names @("categories") -Default @())) -contains $categoryId
        })
        $doc = [ordered]@{
            schema = "skillhub.index.category.v1"
            generatedAt = $script:Now
            category = $categoryId
            skills = @($items)
        }

        $path = Join-Path $WorkDir ("index-category-" + $categoryId + ".json")
        Save-Json -Path $path -Object $doc

        $objectPath = Join-ObjectPath "indexes" "category" ($categoryId + ".json")
        Invoke-Mc -McArgs @("cp", $path, "$Alias/$Bucket/$objectPath")
        $uploaded += $objectPath
    }

    return $uploaded
}

function Build-SearchLiteIndex {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Catalog,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $skills = Normalize-Array (Get-JsonPropertyValue -Object $Catalog -Names @("skills") -Default @())
    $items = @($skills | ForEach-Object {
        $namespace = Get-JsonPropertyValue -Object $_ -Names @("namespace")
        $skillId = Get-JsonPropertyValue -Object $_ -Names @("id")
        [ordered]@{
            key = "$namespace/$skillId"
            namespace = $namespace
            id = $skillId
            name = Get-JsonPropertyValue -Object $_ -Names @("name")
            summary = Get-JsonPropertyValue -Object $_ -Names @("summary")
            latestVersion = Get-JsonPropertyValue -Object $_ -Names @("latestVersion", "latest_version")
            categories = @(Normalize-Array (Get-JsonPropertyValue -Object $_ -Names @("categories") -Default @()))
            tags = @(Normalize-Array (Get-JsonPropertyValue -Object $_ -Names @("tags") -Default @()))
            manifestPath = Get-JsonPropertyValue -Object $_ -Names @("manifestPath", "manifest_path")
        }
    })

    $doc = [ordered]@{
        schema = "skillhub.search-lite.v1"
        generatedAt = $script:Now
        skills = @($items)
    }

    $path = Join-Path $WorkDir "search-lite.json"
    Save-Json -Path $path -Object $doc

    $objectPath = Join-ObjectPath "indexes" "search-lite.json"
    Invoke-Mc -McArgs @("cp", $path, "$Alias/$Bucket/$objectPath")
    return $objectPath
}

function Assert-SkillCategoriesDefined {
    param(
        [Parameter(Mandatory = $true)]
        [object]$CategoriesDoc,

        [Parameter(Mandatory = $true)]
        [object[]]$SkillCategories,

        [Parameter(Mandatory = $true)]
        [string]$CategoriesPath
    )

    $items = @(Normalize-Array (Get-JsonPropertyValue -Object $CategoriesDoc -Names @("items") -Default @()))
    $definedIds = @($items | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })

    foreach ($categoryId in $SkillCategories) {
        Assert-SafeObjectSegment -Name "category id" -Value $categoryId
        $exists = @($definedIds | Where-Object { $_ -eq $categoryId }).Count -gt 0
        if (-not $exists) {
            throw "Category '$categoryId' is used by skill.json but is not defined in $CategoriesPath."
        }
    }

    Set-JsonProperty -Object $CategoriesDoc -Name "schema" -Value "skillhub.categories.v1"
    Set-JsonProperty -Object $CategoriesDoc -Name "generatedAt" -Value $script:Now
}

function Read-CategoriesDoc {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Categories file not found: $Path"
    }

    $doc = Read-JsonFile -Path $Path
    if ($doc.schema -ne "skillhub.categories.v1") {
        throw "Categories file schema must be skillhub.categories.v1: $Path"
    }

    if ($null -eq $doc.items) {
        throw "Categories file must contain an items array: $Path"
    }

    $seenIds = @{}
    foreach ($item in @(Normalize-Array $doc.items)) {
        if ([string]::IsNullOrWhiteSpace($item.id)) {
            throw "Category item in $Path must contain id."
        }
        Assert-SafeObjectSegment -Name "category id" -Value $item.id
        if ($seenIds.ContainsKey($item.id)) {
            throw "Duplicate category id '$($item.id)' in $Path."
        }
        $seenIds[$item.id] = $true

        if ([string]::IsNullOrWhiteSpace($item.name)) {
            throw "Category '$($item.id)' in $Path must contain name."
        }
    }

    return $doc
}

function Resolve-FullPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $Path).Path)
}

$script:Now = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("skillhub-publish-" + [Guid]::NewGuid().ToString("N"))

try {
    if (-not (Get-Command mc -ErrorAction SilentlyContinue)) {
        throw "MinIO Client 'mc' was not found in PATH."
    }

    $skillDirFull = Resolve-FullPath -Path $SkillDir
    if ([string]::IsNullOrWhiteSpace($CategoriesPath)) {
        $CategoriesPath = Join-Path $PSScriptRoot $DefaultCategoriesFileName
    }
    if (-not (Test-Path -LiteralPath $CategoriesPath)) {
        throw "Categories JSON file not found: $CategoriesPath"
    }
    $CategoriesPath = Resolve-FullPath -Path $CategoriesPath

    $skillMdPath = Join-Path $skillDirFull "SKILL.md"
    if (-not (Test-Path -LiteralPath $skillMdPath)) {
        throw "Missing required file: $skillMdPath"
    }

    $readmePath = Join-Path $skillDirFull "README.md"
    $sourceSkillJsonPath = Join-Path $skillDirFull $VersionSkillFileName

    New-Item -ItemType Directory -Path $workDir -Force | Out-Null

    if (Test-Path -LiteralPath $sourceSkillJsonPath) {
        $sourceSkillJson = Read-JsonFile -Path $sourceSkillJsonPath
    }
    else {
        Write-Step "No skill.json found; generating publish metadata from SKILL.md and directory name."
        $directoryName = Split-Path -Leaf $skillDirFull
        $detectedName = Get-MarkdownTitle -Path $skillMdPath
        if ([string]::IsNullOrWhiteSpace($detectedName)) {
            $detectedName = $directoryName
        }

        $detectedSummary = Get-MarkdownSummary -Path $readmePath
        if ([string]::IsNullOrWhiteSpace($detectedSummary)) {
            $detectedSummary = Get-MarkdownSummary -Path $skillMdPath
        }
        if ([string]::IsNullOrWhiteSpace($detectedSummary)) {
            $detectedSummary = "Skill published by Skill Hub."
        }

        $sourceSkillJson = [pscustomobject][ordered]@{
            schema = "skillhub.skill.v1"
            namespace = "local"
            id = Convert-ToSafeObjectSegment -Value $directoryName -Fallback "skill"
            name = $detectedName
            version = "1.0.0"
            summary = $detectedSummary
            categories = @("public")
            tags = @()
            levels = @("personal", "project")
        }
    }

    if ([string]::IsNullOrWhiteSpace($Namespace)) {
        $Namespace = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("namespace") -Default "local"
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("version") -Default "1.0.0"
    }

    $script:Version = $Version

    $skillId = $SkillId
    if ([string]::IsNullOrWhiteSpace($skillId)) {
        $skillId = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("id")
    }
    if ([string]::IsNullOrWhiteSpace($skillId)) {
        $skillId = Convert-ToSafeObjectSegment -Value (Split-Path -Leaf $skillDirFull) -Fallback "skill"
    }

    $skillName = $SkillName
    if ([string]::IsNullOrWhiteSpace($skillName)) {
        $skillName = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("name") -Default $skillId
    }

    $summary = $SkillSummary
    if ([string]::IsNullOrWhiteSpace($summary)) {
        $summary = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("summary") -Default ""
    }

    $categorySource = Get-JsonPropertyValue -Object $sourceSkillJson -Names @("categories") -Default @("public")
    if ($null -ne $SkillCategories -and $SkillCategories.Count -gt 0) {
        $categorySource = $SkillCategories
    }

    $categories = @(Normalize-Array $categorySource)
    $tags = @(Normalize-Array (Get-JsonPropertyValue -Object $sourceSkillJson -Names @("tags") -Default @()))
    $levels = @(Normalize-Array (Get-JsonPropertyValue -Object $sourceSkillJson -Names @("levels") -Default @("personal", "project")))

    if ([string]::IsNullOrWhiteSpace($Namespace)) {
        throw "Namespace is required. Pass -Namespace or set namespace in skill.json."
    }

    if ([string]::IsNullOrWhiteSpace($skillId)) {
        throw "skill.json must contain id."
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        throw "Version is required. Pass -Version or set version in skill.json."
    }

    if ([string]::IsNullOrWhiteSpace($skillName)) {
        $skillName = $skillId
    }

    if ($null -eq $summary) {
        $summary = ""
    }

    if ($categories.Count -eq 0) {
        $categories = @("public")
    }

    if ($levels.Count -eq 0) {
        $levels = @("personal", "project")
    }

    Assert-SafeObjectSegment -Name "namespace" -Value $Namespace
    Assert-SafeObjectSegment -Name "skill id" -Value $skillId
    Assert-SafeObjectSegment -Name "version" -Value $Version

    foreach ($category in $categories) {
        Assert-SafeObjectSegment -Name "category id" -Value $category
    }

    if (-not [string]::IsNullOrWhiteSpace($Endpoint)) {
        if ([string]::IsNullOrWhiteSpace($AccessKey) -or [string]::IsNullOrWhiteSpace($SecretKey)) {
            throw "When -Endpoint is provided, -AccessKey and -SecretKey are required."
        }

        $aliasArgs = @("alias", "set", $Alias, $Endpoint, $AccessKey, $SecretKey, "--api", "S3v4")
        if (-not [string]::IsNullOrWhiteSpace($Region)) {
            $aliasArgs += @("--region", $Region)
        }

        Write-Step "Configuring mc alias '$Alias'."
        Invoke-Mc -McArgs $aliasArgs
    }

    if ($CreateBucket) {
        Write-Step "Ensuring bucket '$Bucket' exists."
        Invoke-Mc -McArgs @("mb", "--ignore-existing", "$Alias/$Bucket")
        Write-Step "Ensuring bucket '$Bucket' allows anonymous downloads."
        Invoke-Mc -McArgs @("anonymous", "set", "download", "$Alias/$Bucket")
    }

    $skillBaseObject = Join-ObjectPath "skills" $Namespace $skillId
    $versionBaseObject = Join-ObjectPath $skillBaseObject "versions" $Version
    $manifestObject = Join-ObjectPath $skillBaseObject "manifest.json"
    $versionSkillObject = Join-ObjectPath $versionBaseObject $VersionSkillFileName
    $packageObject = Join-ObjectPath $versionBaseObject $PackageFileName
    $hashObject = Join-ObjectPath $versionBaseObject $HashFileName
    $changelogObject = Join-ObjectPath $versionBaseObject $ChangelogFileName
    $signatureObject = Join-ObjectPath $versionBaseObject $SignatureFileName

    if ((Test-RemoteObject -ObjectPath $packageObject) -and -not $AllowOverwrite) {
        throw "Remote package already exists: $packageObject. Use -AllowOverwrite to replace it."
    }

    Write-Step "Packaging skill '$Namespace/$skillId@$Version'."
    $packagePath = Join-Path $workDir $PackageFileName
    $payloadDir = Join-Path $workDir "payload"
    Copy-PackagePayload -SourceDir $skillDirFull -DestinationDir $payloadDir -SignatureFileName $SignatureFileName
    $sourceItems = @(Get-ChildItem -LiteralPath $payloadDir -Force)

    if ($sourceItems.Count -eq 0) {
        throw "Skill package payload is empty after filtering json files: $skillDirFull"
    }

    Compress-Archive -Path @($sourceItems | ForEach-Object { $_.FullName }) -DestinationPath $packagePath -Force

    $packageHash = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageSize = (Get-Item -LiteralPath $packagePath).Length
    $hashPath = Join-Path $workDir $HashFileName
    Write-Utf8NoBom -Path $hashPath -Text ($packageHash + [Environment]::NewLine)

    $generatedSkillJson = Copy-JsonObject -Object $sourceSkillJson
    Set-JsonProperty -Object $generatedSkillJson -Name "schema" -Value "skillhub.skill.v1"
    Set-JsonProperty -Object $generatedSkillJson -Name "namespace" -Value $Namespace
    Set-JsonProperty -Object $generatedSkillJson -Name "id" -Value $skillId
    Set-JsonProperty -Object $generatedSkillJson -Name "name" -Value $skillName
    Set-JsonProperty -Object $generatedSkillJson -Name "version" -Value $Version
    Set-JsonProperty -Object $generatedSkillJson -Name "summary" -Value $summary
    Set-JsonProperty -Object $generatedSkillJson -Name "categories" -Value @($categories)
    Set-JsonProperty -Object $generatedSkillJson -Name "tags" -Value @($tags)
    Set-JsonProperty -Object $generatedSkillJson -Name "levels" -Value @($levels)
    Set-JsonProperty -Object $generatedSkillJson -Name "package" -Value ([ordered]@{
        file = $PackageFileName
        path = $packageObject
        sha256 = $packageHash
        sha256Path = $hashObject
        size = $packageSize
    })

    $generatedSkillJsonPath = Join-Path $workDir $VersionSkillFileName
    Save-Json -Path $generatedSkillJsonPath -Object $generatedSkillJson

    if ([string]::IsNullOrWhiteSpace($ChangelogPath)) {
        $ChangelogPath = Find-DefaultChangelog -SkillDir $skillDirFull
    }

    $generatedChangelogPath = $null
    if ([string]::IsNullOrWhiteSpace($ChangelogPath)) {
        $generatedChangelogPath = Join-Path $workDir $ChangelogFileName
        Write-Utf8NoBom -Path $generatedChangelogPath -Text ("# $Version" + [Environment]::NewLine + [Environment]::NewLine + "- Published by Skill Hub publisher." + [Environment]::NewLine)
        $ChangelogPath = $generatedChangelogPath
    }
    else {
        $ChangelogPath = Resolve-FullPath -Path $ChangelogPath
    }

    if ([string]::IsNullOrWhiteSpace($SignaturePath)) {
        $SignaturePath = Find-DefaultSignature -SkillDir $skillDirFull
    }
    elseif (-not (Test-Path -LiteralPath $SignaturePath)) {
        throw "Signature file not found: $SignaturePath"
    }

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        $SignaturePath = Resolve-FullPath -Path $SignaturePath
    }

    Write-Step "Loading remote manifest, catalog, and categories from '$CategoriesPath'."
    $remoteManifest = Get-RemoteJson `
        -ObjectPath $manifestObject `
        -DefaultObject (New-ManifestDefault -Namespace $Namespace -SkillId $skillId -Name $skillName -Summary $summary -Categories $categories -Tags $tags -Levels $levels) `
        -WorkDir $workDir

    $existingVersions = @(Normalize-Array $remoteManifest.versions | ForEach-Object {
        Convert-ToCanonicalVersionEntry -Entry $_
    })
    $existingVersionCount = @($existingVersions | Where-Object { $_.version -eq $Version }).Count
    if ($existingVersionCount -gt 0 -and -not $AllowOverwrite) {
        throw "Version already exists in manifest: $Namespace/$skillId@$Version. Use -AllowOverwrite to replace it."
    }

    $versionEntry = [ordered]@{
        version = $Version
        skillPath = $versionSkillObject
        packagePath = $packageObject
        sha256Path = $hashObject
        changelogPath = $changelogObject
        createdAt = $script:Now
        package = [ordered]@{
            file = $PackageFileName
            sha256 = $packageHash
            size = $packageSize
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        $versionEntry["signaturePath"] = $signatureObject
    }

    $updatedVersions = @($existingVersions | Where-Object { $_.version -ne $Version })
    $updatedVersions += $versionEntry

    $manifest = New-ManifestDefault -Namespace $Namespace -SkillId $skillId -Name $skillName -Summary $summary -Categories $categories -Tags $tags -Levels $levels
    Set-JsonProperty -Object $manifest -Name "versions" -Value @($updatedVersions)

    $manifestPath = Join-Path $workDir "manifest.json"
    Save-Json -Path $manifestPath -Object $manifest

    $categoriesDoc = Read-CategoriesDoc -Path $CategoriesPath
    Assert-SkillCategoriesDefined -CategoriesDoc $categoriesDoc -SkillCategories $categories -CategoriesPath $CategoriesPath
    $categoriesDoc = Convert-ToCanonicalCategoriesDoc -CategoriesDoc $categoriesDoc

    $remoteCatalog = Get-RemoteJson `
        -ObjectPath "catalog.v1.json" `
        -DefaultObject (New-CatalogDefault) `
        -WorkDir $workDir

    $skillEntry = New-SkillEntry `
        -Namespace $Namespace `
        -SkillId $skillId `
        -Name $skillName `
        -Summary $summary `
        -Version $Version `
        -Categories $categories `
        -Tags $tags `
        -Levels $levels `
        -ManifestPath $manifestObject

    $catalogSkills = @(Normalize-Array (Get-JsonPropertyValue -Object $remoteCatalog -Names @("skills") -Default @()) | ForEach-Object {
        Convert-ToCanonicalSkillEntry -Entry $_
    } | Where-Object { -not ($_.namespace -eq $Namespace -and $_.id -eq $skillId) })
    $catalogSkills += $skillEntry

    $categoryIds = @(Normalize-Array (Get-JsonPropertyValue -Object $categoriesDoc -Names @("items") -Default @()) | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)

    $catalog = New-CatalogDefault
    Set-JsonProperty -Object $catalog -Name "categories" -Value @($categoryIds)
    Set-JsonProperty -Object $catalog -Name "skills" -Value @($catalogSkills)

    $generatedCategoriesPath = Join-Path $workDir "categories.v1.json"
    $catalogPath = Join-Path $workDir "catalog.v1.json"
    Save-Json -Path $generatedCategoriesPath -Object $categoriesDoc
    Save-Json -Path $catalogPath -Object $catalog

    Write-Step "Uploading version files."
    Invoke-Mc -McArgs @("cp", $generatedSkillJsonPath, "$Alias/$Bucket/$versionSkillObject")
    Invoke-Mc -McArgs @("cp", $packagePath, "$Alias/$Bucket/$packageObject")
    Invoke-Mc -McArgs @("cp", $hashPath, "$Alias/$Bucket/$hashObject")
    Invoke-Mc -McArgs @("cp", $ChangelogPath, "$Alias/$Bucket/$changelogObject")

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        Invoke-Mc -McArgs @("cp", $SignaturePath, "$Alias/$Bucket/$signatureObject")
    }

    Write-Step "Uploading skill manifest."
    Invoke-Mc -McArgs @("cp", $manifestPath, "$Alias/$Bucket/$manifestObject")

    Write-Step "Uploading categories and indexes."
    Invoke-Mc -McArgs @("cp", $generatedCategoriesPath, "$Alias/$Bucket/categories.v1.json")
    $categoryIndexObjects = Build-CategoryIndexes -Catalog $catalog -CategoriesDoc $categoriesDoc -WorkDir $workDir
    $searchIndexObject = Build-SearchLiteIndex -Catalog $catalog -WorkDir $workDir
    Invoke-Mc -McArgs @("rm", "--recursive", "--force", "$Alias/$Bucket/indexes/target")

    Write-Step "Uploading catalog last."
    Invoke-Mc -McArgs @("cp", $catalogPath, "$Alias/$Bucket/catalog.v1.json")

    Write-Host ""
    Write-Host "Published: $Namespace/$skillId@$Version"
    Write-Host "Package:   $packageObject"
    Write-Host "SHA-256:   $packageHash"
    Write-Host "Manifest:  $manifestObject"
    Write-Host "Catalog:   catalog.v1.json"
    Write-Host "Indexes:   $($categoryIndexObjects.Count) category, search-lite"
    if ($DryRun) {
        Write-Host "Mode:      dry run; no objects were uploaded"
    }
}
finally {
    if ($KeepTemp) {
        Write-Host "Temp kept: $workDir"
    }
    elseif (Test-Path -LiteralPath $workDir) {
        Remove-Item -LiteralPath $workDir -Recurse -Force
    }
}
