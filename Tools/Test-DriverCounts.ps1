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
$work=Join-Path $workspace ('Temp/KernelDriverCounts/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 & $starter cargo build --manifest-path (Join-Path $contract 'Generated/SDK/Rust/Cargo.toml') --target-dir (Join-Path $work 'contract')
 if($LASTEXITCODE -ne 0){throw 'Current Contract build failed.'}
 $library=@(Get-ChildItem -LiteralPath (Join-Path $work 'contract/debug/deps') -Filter 'libr4os_contract-*.rlib')
 if($library.Count -ne 1){throw 'Expected one exact Contract library.'}
 $source=Join-Path $work 'counts.rs';$executable=Join-Path $work $(if($IsWindows){'counts.exe'}else{'counts'})
 $body="#![allow(dead_code)]`npub use r4os_contract as contract;`nmod driver {`n"
 foreach($name in @('owner_table','owner_storage','treap','memory_owner','thread_owner','semaphore_owner','resource_state')){
  $file=(Join-Path $kernel ('Rust/driver/'+$name+'.rs')).Replace('\','/')
  $body+="#[path = `"$file`"] pub mod $name;`n"
 }
 $fixture=(Join-Path $kernel 'Tools/Fixtures/DriverCounts.rs').Replace('\','/')
 $body+="}`n#[path = `"$fixture`"] mod checks;`npub use checks::{arch,memory,DEPTH,FAIL,ALLOCS,Ordering};`n"
 [IO.File]::WriteAllText($source,$body)
 & $starter rustc --edition 2024 --test $source --extern ('r4os_contract='+$library[0].FullName) -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual driver owner algorithms did not compile.'}
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Actual driver owner growth/failure checks failed.'}
}finally{if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}}
