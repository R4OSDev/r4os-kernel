param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$settings=@{}
foreach($line in [IO.File]::ReadAllLines((Join-Path $kernel 'Settings.R4S'))){
 $line=$line.Trim().TrimStart([char]0xFEFF)
 if($line -and !$line.StartsWith('#')){$key,$value=$line.Split('=',2);$settings[$key]=$value}
}
$workspace=[IO.Path]::GetFullPath((Join-Path $kernel $settings.WORKSPACE_ROOT.Replace('\',[IO.Path]::DirectorySeparatorChar)))
$devkit=[IO.Path]::GetFullPath((Join-Path $workspace $settings.DEVKIT_ROOT.Replace('\',[IO.Path]::DirectorySeparatorChar)))
$starter=Join-Path $devkit 'Setup/Rust.ps1'
$work=Join-Path $workspace ('Temp/KernelLoaderRecords/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 $records=(Join-Path $kernel 'Rust/loader/r4m.rs').Replace('\','/')
 $stats=(Join-Path $kernel 'Rust/loader/file_stats.rs').Replace('\','/')
 $source=Join-Path $work 'records.rs';$executable=Join-Path $work $(if($IsWindows){'records.exe'}else{'records'})
 $body=@'
#![allow(dead_code)]
mod fs { pub mod vfs_types { #[repr(C)] #[derive(Clone, Copy)] pub struct Bytes { pub data: *const u8, pub len: usize } } }
mod file { pub const WINDOW_SIZE: usize = 4096; }
'@
 [IO.File]::WriteAllText($source,$body+"`n#[path = `"$records`"] mod r4m;`n#[path = `"$stats`"] mod file_stats;`n")
 & $starter rustc --edition 2024 --test $source -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual loader record owners did not compile.'}
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Actual loader owner checks failed.'}
}finally{
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}
}
