param([string]$Filter="")
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
$work=Join-Path $workspace ('Temp/KernelRtc/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 $rtc=(Join-Path $kernel 'Rust/arch/rtc.rs').Replace('\','/')
 $ntfsTime=(Join-Path $kernel 'Rust/fs/ntfs_adapter_time.rs').Replace('\','/')
 $ports=(Join-Path $kernel 'Tools/Fixtures/RtcPorts.rs').Replace('\','/')
 $source=Join-Path $work 'rtc.rs';$executable=Join-Path $work $(if($IsWindows){'rtc.exe'}else{'rtc'})
 $body=@"
#![allow(dead_code)]
#[path = "$ports"] mod fixture;
mod memory { pub mod owner { pub use crate::fixture::{Guard,rtc_guard}; } }
mod platform {
    pub mod monotonic { pub use crate::fixture::now_nanoseconds; }
    pub mod time { pub fn calendar() -> crate::arch::rtc::DateTime { crate::arch::rtc::read_date_time(0x32) } }
}
mod fs { #[path = "$ntfsTime"] mod ntfs_adapter_time; }
mod arch {
    pub mod io { pub use crate::fixture::{inb,outb}; }
    #[path = "$rtc"] pub mod rtc;
}
"@
 [IO.File]::WriteAllText($source,$body)
 & $starter rustc --edition 2024 --test $source -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual RTC owners did not compile.'}
 & $executable --test-threads=1 $Filter
 if($LASTEXITCODE -ne 0){throw 'Actual RTC owner checks failed.'}
}finally{
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}
}
