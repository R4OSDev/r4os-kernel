param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$settings=@{}
foreach($line in [IO.File]::ReadAllLines((Join-Path $kernel 'Settings.R4S'))){
 $line=$line.Trim().TrimStart([char]0xFEFF)
 if($line -and !$line.StartsWith('#')){$key,$value=$line.Split('=',2);$settings[$key]=$value}
}
function Resolve-RecordPath([string]$Base,[string]$Relative){[IO.Path]::GetFullPath((Join-Path $Base $Relative.Replace('\',[IO.Path]::DirectorySeparatorChar)))}
$workspace=Resolve-RecordPath $kernel $settings.WORKSPACE_ROOT
$repositories=Resolve-RecordPath $kernel $settings.REPOSITORIES_ROOT
$contract=Resolve-RecordPath $repositories $settings.CONTRACT_ROOT
$devkit=Resolve-RecordPath $workspace $settings.DEVKIT_ROOT
$starter=Join-Path $devkit 'Setup/Rust.ps1'
$work=Join-Path $workspace ('Temp/KernelGraphicsRecords/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 & $starter cargo build --manifest-path (Join-Path $contract 'Generated/SDK/Rust/Cargo.toml') --target-dir (Join-Path $work 'contract')
 if($LASTEXITCODE -ne 0){throw 'Current Contract build failed.'}
 $library=@(Get-ChildItem -LiteralPath (Join-Path $work 'contract/debug/deps') -Filter 'libr4os_contract-*.rlib')
 if($library.Count -ne 1){throw 'Expected one exact Contract library.'}
 $wire=(Join-Path $kernel 'Rust/display/wire.rs').Replace('\','/')
 $stats=(Join-Path $kernel 'Rust/gfx/stats_wire.rs').Replace('\','/')
 $source=Join-Path $work 'records.rs';$executable=Join-Path $work $(if($IsWindows){'records.exe'}else{'records'})
 [IO.File]::WriteAllText($source,"#![allow(dead_code)]`npub use r4os_contract as contract;`n#[path = `"$wire`"] mod wire;`n#[path = `"$stats`"] mod stats_wire;`n")
 & $starter rustc --edition 2024 --test $source --extern ('r4os_contract='+$library[0].FullName) -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual graphics record owners did not compile.'}
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Actual graphics record owner checks failed.'}
 Write-Output (@{status='pass';owner_tests=2;software_only=$true;wire_sha256=(Get-FileHash -LiteralPath $wire -Algorithm SHA256).Hash.ToLowerInvariant();stats_sha256=(Get-FileHash -LiteralPath $stats -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json -Compress)
}finally{
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}
}
