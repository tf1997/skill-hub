<#
.SYNOPSIS
Seeds a local MinIO bucket with Skill Hub demo marketplace data.

.EXAMPLE
.\seed-minio.ps1 -McPath D:\tmp\skillhub-minio\mc.exe -Alias skillhub -Bucket skill-market
#>

[CmdletBinding()]
param(
    [string]$McPath = "mc",
    [string]$Alias = "skillhub",
    [string]$Bucket = "skill-market"
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
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

function Invoke-Mc {
    param([Parameter(Mandatory = $true)][string[]]$Args)
    & $McPath @Args
    if ($LASTEXITCODE -ne 0) {
        throw "mc failed: $($Args -join ' ')"
    }
}

function Join-ObjectPath {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Parts)
    return (($Parts | Where-Object { $_ } | ForEach-Object { $_.Trim("/") }) -join "/")
}

function Copy-ToMinio {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LocalPath,

        [Parameter(Mandatory = $true)]
        [string]$ObjectPath
    )

    Invoke-Mc -Args @("cp", $LocalPath, "$Alias/$Bucket/$ObjectPath")
}

function Copy-PackagePayload {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceDir,

        [Parameter(Mandatory = $true)]
        [string]$DestinationDir
    )

    New-Item -ItemType Directory -Path $DestinationDir -Force | Out-Null
    $sourceRoot = (Resolve-Path -LiteralPath $SourceDir).Path

    Get-ChildItem -LiteralPath $sourceRoot -Recurse -Force | Where-Object {
        -not $_.PSIsContainer -and
        $_.Extension -ne ".json" -and
        $_.Name -ne ".DS_Store"
    } | ForEach-Object {
        $relative = $_.FullName.Substring($sourceRoot.Length).TrimStart("\", "/")
        $target = Join-Path $DestinationDir $relative
        $parent = Split-Path -Parent $target
        if (-not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        Copy-Item -LiteralPath $_.FullName -Destination $target -Force
    }
}

function New-SkillSeed {
    param(
        [string]$Dir,
        [string]$Namespace,
        [string]$Id,
        [string]$Name,
        [string]$Version,
        [string]$Summary,
        [string[]]$Categories,
        [string[]]$Tags
    )

    return [ordered]@{
        dir = $Dir
        namespace = $Namespace
        id = $Id
        name = $Name
        version = $Version
        summary = $Summary
        categories = @($Categories)
        tags = @($Tags)
        levels = @("personal", "project")
    }
}

$now = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("skillhub-seed-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $workDir -Force | Out-Null

$skills = @(
    (New-SkillSeed `
        -Dir "examples/frontend-reviewer" `
        -Namespace "official" `
        -Id "frontend-reviewer" `
        -Name "Frontend Reviewer" `
        -Version "1.0.0" `
        -Summary "Review frontend components for usability, accessibility, and visual consistency." `
        -Categories @("frontend") `
        -Tags @("react", "review", "accessibility")),
    (New-SkillSeed `
        -Dir "examples/api-contract-writer" `
        -Namespace "official" `
        -Id "api-contract-writer" `
        -Name "API Contract Writer" `
        -Version "1.0.0" `
        -Summary "Draft API contracts, edge cases, and test scenarios for backend services." `
        -Categories @("backend") `
        -Tags @("api", "openapi", "testing")),
    (New-SkillSeed `
        -Dir "examples/prd-shaper" `
        -Namespace "community" `
        -Id "prd-shaper" `
        -Name "PRD Shaper" `
        -Version "0.9.0" `
        -Summary "Turn raw product ideas into structured PRDs, acceptance criteria, and review notes." `
        -Categories @("product") `
        -Tags @("prd", "planning", "acceptance"))
)

try {
    Invoke-Mc -Args @("mb", "--ignore-existing", "$Alias/$Bucket")
    Invoke-Mc -Args @("anonymous", "set", "download", "$Alias/$Bucket")

    $catalogSkills = @()

    foreach ($skill in $skills) {
        $skillDir = Resolve-Path -LiteralPath $skill.dir
        $skillWorkDir = Join-Path $workDir ($skill.namespace + "." + $skill.id)
        New-Item -ItemType Directory -Path $skillWorkDir -Force | Out-Null

        $packagePath = Join-Path $skillWorkDir "package.zip"
        $payloadDir = Join-Path $skillWorkDir "payload"
        Copy-PackagePayload -SourceDir $skillDir -DestinationDir $payloadDir
        $items = Get-ChildItem -LiteralPath $payloadDir -Force
        if ($items.Count -eq 0) {
            throw "No non-json package payload found: $skillDir"
        }
        Compress-Archive -Path @($items | ForEach-Object { $_.FullName }) -DestinationPath $packagePath -Force

        $sha256 = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash.ToLowerInvariant()
        $shaPath = Join-Path $skillWorkDir "package.sha256"
        Write-Utf8NoBom -Path $shaPath -Text ($sha256 + [Environment]::NewLine)

        $baseObject = Join-ObjectPath "skills" $skill.namespace $skill.id
        $versionObject = Join-ObjectPath $baseObject "versions" $skill.version
        $skillObjectPath = Join-ObjectPath $versionObject "skill.json"
        $packageObjectPath = Join-ObjectPath $versionObject "package.zip"
        $shaObjectPath = Join-ObjectPath $versionObject "package.sha256"
        $changelogObjectPath = Join-ObjectPath $versionObject "changelog.md"
        $manifestObjectPath = Join-ObjectPath $baseObject "manifest.json"

        $skillJson = [ordered]@{
            schema = "skillhub.skill.v1"
            id = $skill.id
            namespace = $skill.namespace
            name = $skill.name
            version = $skill.version
            summary = $skill.summary
            categories = @($skill.categories)
            tags = @($skill.tags)
            levels = @($skill.levels)
            author = [ordered]@{ name = "Skill Hub" }
            license = "MIT"
            compatibility = [ordered]@{}
            permissions = [ordered]@{
                network = $false
                filesystem = "project-read"
            }
            package = [ordered]@{
                file = "package.zip"
                path = $packageObjectPath
                sha256 = $sha256
                sha256_path = $shaObjectPath
                size = (Get-Item -LiteralPath $packagePath).Length
            }
        }

        $skillJsonPath = Join-Path $skillWorkDir "skill.json"
        Save-Json -Path $skillJsonPath -Object $skillJson

        $manifest = [ordered]@{
            schema = "skillhub.skill-manifest.v1"
            namespace = $skill.namespace
            id = $skill.id
            name = $skill.name
            summary = $skill.summary
            categories = @($skill.categories)
            tags = @($skill.tags)
            levels = @($skill.levels)
            latest_version = $skill.version
            versions = @(
                [ordered]@{
                    version = $skill.version
                    skill_path = $skillObjectPath
                    package_path = $packageObjectPath
                    sha256_path = $shaObjectPath
                    changelog_path = $changelogObjectPath
                    created_at = $now
                    package = [ordered]@{
                        file = "package.zip"
                        sha256 = $sha256
                        size = (Get-Item -LiteralPath $packagePath).Length
                    }
                }
            )
            updated_at = $now
        }

        $manifestPath = Join-Path $skillWorkDir "manifest.json"
        Save-Json -Path $manifestPath -Object $manifest

        Copy-ToMinio -LocalPath $skillJsonPath -ObjectPath $skillObjectPath
        Copy-ToMinio -LocalPath $packagePath -ObjectPath $packageObjectPath
        Copy-ToMinio -LocalPath $shaPath -ObjectPath $shaObjectPath
        Copy-ToMinio -LocalPath (Join-Path $skillDir "changelog.md") -ObjectPath $changelogObjectPath
        Copy-ToMinio -LocalPath $manifestPath -ObjectPath $manifestObjectPath

        $catalogSkills += [ordered]@{
            namespace = $skill.namespace
            id = $skill.id
            name = $skill.name
            summary = $skill.summary
            latest_version = $skill.version
            categories = @($skill.categories)
            tags = @($skill.tags)
            levels = @($skill.levels)
            manifest_path = $manifestObjectPath
            updated_at = $now
        }
    }

    $categories = @(
        [ordered]@{ id = "public"; name = "Public"; order = 10 },
        [ordered]@{ id = "frontend"; name = "Frontend"; order = 20 },
        [ordered]@{ id = "backend"; name = "Backend"; order = 30 },
        [ordered]@{ id = "product"; name = "Product"; order = 40 }
    )

    $categoriesDoc = [ordered]@{
        schema = "skillhub.categories.v1"
        generated_at = $now
        items = @($categories)
    }
    $categoriesPath = Join-Path $workDir "categories.v1.json"
    Save-Json -Path $categoriesPath -Object $categoriesDoc
    Copy-ToMinio -LocalPath $categoriesPath -ObjectPath "categories.v1.json"

    foreach ($category in $categories) {
        $items = @($catalogSkills | Where-Object { $_.categories -contains $category.id })
        $index = [ordered]@{
            schema = "skillhub.index.category.v1"
            generated_at = $now
            category = $category.id
            skills = @($items)
        }
        $path = Join-Path $workDir ("category-" + $category.id + ".json")
        Save-Json -Path $path -Object $index
        Copy-ToMinio -LocalPath $path -ObjectPath (Join-ObjectPath "indexes" "category" ($category.id + ".json"))
    }

    $searchLite = [ordered]@{
        schema = "skillhub.search-lite.v1"
        generated_at = $now
        skills = @($catalogSkills)
    }
    $searchPath = Join-Path $workDir "search-lite.json"
    Save-Json -Path $searchPath -Object $searchLite
    Copy-ToMinio -LocalPath $searchPath -ObjectPath "indexes/search-lite.json"

    Invoke-Mc -Args @("rm", "--recursive", "--force", "$Alias/$Bucket/indexes/target")

    $catalogDoc = [ordered]@{
        schema = "skillhub.catalog.v1"
        generated_at = $now
        categories = @("public", "frontend", "backend", "product")
        skills = @($catalogSkills)
    }
    $catalogPath = Join-Path $workDir "catalog.v1.json"
    Save-Json -Path $catalogPath -Object $catalogDoc
    Copy-ToMinio -LocalPath $catalogPath -ObjectPath "catalog.v1.json"

    Write-Host "Seeded Skill Hub marketplace:"
    Write-Host "  Endpoint: http://127.0.0.1:9000"
    Write-Host "  Console:  http://127.0.0.1:9001"
    Write-Host "  Bucket:   $Bucket"
    Write-Host "  Skills:   $($skills.Count)"
}
finally {
    if (Test-Path -LiteralPath $workDir) {
        Remove-Item -LiteralPath $workDir -Recurse -Force
    }
}
