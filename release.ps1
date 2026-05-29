#!/usr/bin/env pwsh
#Requires -Version 5.0

<#
.SYNOPSIS
    Cavvy 发布脚本 - 构建Release版并生成多个压缩包

.DESCRIPTION
    1. 构建Release版项目
    2. 删除所有.gdb, .d等调试文件
    3. 生成3个不同配置的压缩包
#>

$ErrorActionPreference = "Stop"

# 颜色定义
$ColorInfo = "Cyan"
$ColorSuccess = "Green"
$ColorWarning = "Yellow"
$ColorError = "Red"

# 获取版本号
function Get-Version {
    $verinfo = Get-Content "./.verinfo" -Raw
    if ($verinfo -match 'version\s*=\s*"([^"]+)"') {
        return $matches[1]
    }
    throw "无法从.verinfo解析版本号"
}

# 清理调试文件
function Remove-DebugFiles {
    param([string]$Path)

    Write-Host "🧹 清理调试文件..." -ForegroundColor $ColorInfo

    $debugPatterns = @("*.gdb", "*.d", "*.pdb", "*.exp", "*.ilk", "*.lib", "*.a", "*.o", "*.obj")
    $removedCount = 0

    foreach ($pattern in $debugPatterns) {
        $files = Get-ChildItem -Path $Path -Filter $pattern -Recurse -ErrorAction SilentlyContinue
        foreach ($file in $files) {
            try {
                Remove-Item $file.FullName -Force -ErrorAction SilentlyContinue
                $removedCount++
            } catch {
                # 忽略删除错误
            }
        }
    }

    Write-Host "   已清理 $removedCount 个调试文件" -ForegroundColor $ColorSuccess
}

# 创建临时目录结构
function New-ReleaseStructure {
    param(
        [string]$TempDir,
        [string[]]$IncludeDirs,
        [string[]]$IncludeFiles
    )

    # 创建临时目录
    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force $TempDir
    }
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

    # 复制目录
    foreach ($dir in $IncludeDirs) {
        if (Test-Path $dir) {
            $dest = Join-Path $TempDir $dir
            Copy-Item -Path $dir -Destination $dest -Recurse -Force
        }
    }

    # 复制文件
    foreach ($file in $IncludeFiles) {
        if (Test-Path $file) {
            Copy-Item -Path $file -Destination $TempDir -Force
        }
    }

    return $TempDir
}

# 创建压缩包
function New-ReleaseZip {
    param(
        [string]$SourceDir,
        [string]$ZipFile
    )

    Write-Host "📦 创建压缩包: $ZipFile" -ForegroundColor $ColorInfo

    # 如果文件已存在，先删除
    if (Test-Path $ZipFile) {
        Remove-Item -Force $ZipFile
    }

    # 使用Compress-Archive创建zip
    Compress-Archive -Path "$SourceDir\*" -DestinationPath $ZipFile -Force

    $size = (Get-Item $ZipFile).Length / 1MB
    Write-Host "   完成: $([math]::Round($size, 2)) MB" -ForegroundColor $ColorSuccess
}

# 主函数
function Main {
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor $ColorInfo
    Write-Host "           Cavvy 发布脚本" -ForegroundColor $ColorInfo
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor $ColorInfo

    # 获取版本号
    $version = Get-Version
    Write-Host "📋 版本号: $version" -ForegroundColor $ColorInfo

    # 检查发布目录
    $releaseDir = ".\target\release"
    if (!(Test-Path $releaseDir)) {
        Write-Host "❌ 错误: 未找到Release构建目录 $releaseDir" -ForegroundColor $ColorError
        Write-Host "   请先运行: cargo build --release" -ForegroundColor $ColorWarning
        exit 1
    }

    # 检查必要的可执行文件
    $requiredBins = @("cayc.exe", "cay-ir.exe", "ir2exe.exe", "cay-run.exe", "cay-rcpl.exe", "cay-lsp.exe", "cay-check.exe", "cay-bcgen.exe", "cay-pre.exe")
    $missingBins = @()
    foreach ($bin in $requiredBins) {
        $binPath = Join-Path $releaseDir $bin
        if (!(Test-Path $binPath)) {
            $missingBins += $bin
        }
    }

    if ($missingBins.Count -gt 0) {
        Write-Host "❌ 错误: 缺少以下可执行文件:" -ForegroundColor $ColorError
        foreach ($bin in $missingBins) {
            Write-Host "   - $bin" -ForegroundColor $ColorError
        }
        Write-Host "   请先运行: cargo build --release" -ForegroundColor $ColorWarning
        exit 1
    }

    # 创建发布目录
    $distDir = ".\dist"
    if (!(Test-Path $distDir)) {
        New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    }

    # 所有二进制文件列表（方便维护）
    $allBins = @(
        "$releaseDir\cayc.exe",
        "$releaseDir\cay-ir.exe",
        "$releaseDir\ir2exe.exe",
        "$releaseDir\cay-check.exe",
        "$releaseDir\cay-run.exe",
        "$releaseDir\cay-rcpl.exe",
        "$releaseDir\cay-lsp.exe",
        "$releaseDir\cay-bcgen.exe",
        "$releaseDir\cay-pre.exe"
    )

    # ========== 压缩包1: cavvy-版本-win86-64-mingw64-llvm21.zip ==========
    Write-Host ""
    Write-Host "📦 构建压缩包 1/3: cavvy-$version-win86-64-mingw64-llvm21.zip" -ForegroundColor $ColorInfo

    $tempDir1 = "..\temp_release_1"
    $includeDirs1 = @("examples", "third-party", "lib", "caylibs", "llvm-minimal", "mingw-minimal")
    $includeFiles1 = $allBins

    New-ReleaseStructure -TempDir $tempDir1 -IncludeDirs $includeDirs1 -IncludeFiles $includeFiles1
    Remove-DebugFiles -Path $tempDir1
    New-ReleaseZip -SourceDir $tempDir1 -ZipFile "$distDir\cavvy-$version-win86-64-mingw64-llvm21.zip"
    Remove-Item -Recurse -Force $tempDir1

    # ========== 压缩包2: cavvy-版本-win86-64-core.zip ==========
    Write-Host ""
    Write-Host "📦 构建压缩包 2/3: cavvy-$version-win86-64-core.zip" -ForegroundColor $ColorInfo

    $tempDir2 = "..\temp_release_2"
    $includeDirs2 = @("examples", "third-party", "caylibs")
    $includeFiles2 = $allBins

    New-ReleaseStructure -TempDir $tempDir2 -IncludeDirs $includeDirs2 -IncludeFiles $includeFiles2
    Remove-DebugFiles -Path $tempDir2
    New-ReleaseZip -SourceDir $tempDir2 -ZipFile "$distDir\cavvy-$version-win86-64-core.zip"
    Remove-Item -Recurse -Force $tempDir2

    # ========== 压缩包3: cavvy-版本-win86-64-only-lib.zip ==========
    Write-Host ""
    Write-Host "📦 构建压缩包 3/3: cavvy-$version-win86-64-only-lib.zip" -ForegroundColor $ColorInfo

    $tempDir3 = "..\temp_release_3"
    $includeDirs3 = @("examples", "third-party", "lib", "caylibs")
    $includeFiles3 = $allBins

    New-ReleaseStructure -TempDir $tempDir3 -IncludeDirs $includeDirs3 -IncludeFiles $includeFiles3
    Remove-DebugFiles -Path $tempDir3
    New-ReleaseZip -SourceDir $tempDir3 -ZipFile "$distDir\cavvy-$version-win86-64-only-lib.zip"
    Remove-Item -Recurse -Force $tempDir3

    # 完成
    Write-Host ""
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor $ColorSuccess
    Write-Host "           发布完成!" -ForegroundColor $ColorSuccess
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor $ColorSuccess
    Write-Host ""
    Write-Host "📁 输出目录: $(Resolve-Path $distDir)" -ForegroundColor $ColorInfo
    Write-Host ""
    Write-Host "生成的文件:" -ForegroundColor $ColorInfo

    $zipFiles = Get-ChildItem -Path $distDir -Filter "*.zip" | Sort-Object Name
    foreach ($zip in $zipFiles) {
        $size = $zip.Length / 1MB
        Write-Host "   ✓ $($zip.Name) ($([math]::Round($size, 2)) MB)" -ForegroundColor $ColorSuccess
    }

    Write-Host ""
}

# 执行主函数
Main
