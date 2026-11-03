param()
$BuildArguments = @($args)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$kernel = $PSScriptRoot
$settings = @{}
foreach ($line in [IO.File]::ReadAllLines((Join-Path $kernel 'Settings.R4S'))) {
    $line = $line.Trim().TrimStart([char]0xFEFF)
    if (-not $line -or $line.StartsWith('#')) { continue }
    $key,$value = $line.Split('=',2)
    if (-not $value) { throw ('Invalid Kernel setting: '+$key) }
    $settings[$key] = $value
}
function Resolve-KernelPath([string]$Base,[string]$Value) {
    if (-not $Value) { throw 'Missing Kernel path setting.' }
    $Value = $Value.Replace('\',[IO.Path]::DirectorySeparatorChar).Replace('/',[IO.Path]::DirectorySeparatorChar)
    return [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted($Value)) {$Value} else {Join-Path $Base $Value}))
}
$workspace = Resolve-KernelPath $kernel $settings.WORKSPACE_ROOT
$repositories = Resolve-KernelPath $kernel $settings.REPOSITORIES_ROOT
$contract = Resolve-KernelPath $repositories $settings.CONTRACT_ROOT
$devkit = Resolve-KernelPath $workspace $settings.DEVKIT_ROOT
$artifacts = Resolve-KernelPath $workspace $settings.ARTIFACTS_ROOT
$sdk = Resolve-KernelPath $repositories $settings.SDK_ROOT
$libraries = Resolve-KernelPath $repositories $settings.LIBRARIES_ROOT
. (Join-Path $sdk 'Tools/BuildContext.ps1')
$context=Get-R4SdkBuildContext $sdk $contract $libraries $devkit -Profile 'r4os-kernel-rust-v1'
Assert-R4SdkBuildArguments $context @()
foreach($entry in Get-ChildItem Env:){if($entry.Name.StartsWith('CARGO_PROFILE_',[StringComparison]::Ordinal)){throw 'KernelSdkAmbientProfileForbidden'}}
foreach($name in @('RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTDOC','RUSTC_BOOTSTRAP')){if([Environment]::GetEnvironmentVariable($name,'Process')){throw ('KernelSdkAmbientToolForbidden: '+$name)}}
$ancestor=[IO.DirectoryInfo]::new($kernel)
while($null -ne $ancestor){
    foreach($file in @('.cargo/config','.cargo/config.toml')){if(Test-Path -LiteralPath (Join-Path $ancestor.FullName $file)){throw 'KernelSdkCargoConfigForbidden'}}
    $ancestor=$ancestor.Parent
}
foreach($file in @('config','config.toml')){if(Test-Path -LiteralPath (Join-Path $devkit ('Toolchains/Rust/cargo/'+$file))){throw 'KernelSdkCargoConfigForbidden'}}
foreach($entry in Get-ChildItem Env:){if($entry.Name -cmatch '^CARGO_(TARGET_.*_(LINKER|RUSTFLAGS|RUNNER)|BUILD_(RUSTC|RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER))$'){throw 'KernelSdkAmbientTargetForbidden'}}
$codegen=$context.resolved.codegen
$pin=$context.data.Tools.tools.rust
$rust=Join-Path $sdk 'Tools/Rust.ps1'
$runner=@('-NoLogo','-NoProfile','-File',$rust,'--root',$devkit,'--profile','kernel')
& (Join-Path $sdk 'Tools/Sync-KernelProjections.ps1') -KernelRoot $kernel -ContractRoot $contract -LibrariesRoot $libraries
$patches=@('--config',('patch.crates-io.r4os-contract.path='+('"'+(Join-Path $contract 'Generated/SDK/Rust').Replace('\','/')+'"')),
    '--config',('patch.crates-io.r4os-text-core.path='+('"'+(Join-Path $libraries 'R4STD/Rust').Replace('\','/')+'"')))
$specs = @(Get-Content -LiteralPath (Join-Path $kernel 'BuildOptions.json') -Raw | ConvertFrom-Json)
$selected = [ordered]@{}
foreach ($spec in $specs) { $selected[$spec.name] = $spec.default }
$command='build'
foreach ($argument in $BuildArguments) {
    if ($argument -in @('-h','--help')) {
        Write-Host 'Kernel/Build.sh [build|install|check|clippy|fmt] [-DOPTION=VALUE]'
        Write-Host 'The kernel always uses the pinned checked Rust release profile.'
        foreach($spec in $specs){Write-Host ('  -D'+$spec.name+'=VALUE  '+$spec.description)}
        exit 0
    }
    if ($argument -in @('build','install','check','clippy','fmt')) { $command=if($argument -ceq 'install'){'build'}else{$argument}; continue }
    if ($argument -ceq '-Doptimize=ReleaseSafe') { continue }
    if ($argument -notmatch '^-D([^=]+)=(.*)$') { throw ('Unsupported Kernel command or retired Zig test selector: '+$argument) }
    $name,$value = $Matches[1],$Matches[2]
    $spec = @($specs | Where-Object name -CEQ $name)
    if ($spec.Count -ne 1) { throw ('Unknown Kernel option: '+$name) }
    if ($spec[0].type -ceq 'bool') {
        if ($value -cnotin @('true','false')) { throw ('Kernel Boolean needs true/false: '+$name) }
        $selected[$name] = if($value -ceq 'true'){1}else{0}
    } else {
        $number = [uint32]0
        if (-not [uint32]::TryParse($value,[ref]$number)) { throw ('Kernel u32 option is invalid: '+$name) }
        $selected[$name] = $number
    }
}
$manifest=Join-Path $kernel 'Cargo.toml'
# Cargo bindings are SDK-checked projections; exact local roots have already
# been bound to the SDK source set. No separate Git/Contract chooser exists.
$graphText=@(& pwsh @runner cargo metadata --offline --locked --format-version 1 --manifest-path $manifest @patches) -join "`n"
if($LASTEXITCODE -ne 0){throw 'Cannot resolve SDK-selected Kernel bindings.'}
$graph=$graphText|ConvertFrom-Json
foreach($binding in @(@('r4os-contract',(Join-Path $contract 'Generated/SDK/Rust/Cargo.toml')),@('r4os-text-core',(Join-Path $libraries 'R4STD/Rust/Cargo.toml')))){
    $package=@($graph.packages|Where-Object name -CEQ $binding[0])
    if($package.Count -ne 1 -or ![IO.Path]::GetFullPath($package[0].manifest_path).Equals([IO.Path]::GetFullPath($binding[1]),$(if($IsWindows){[StringComparison]::OrdinalIgnoreCase}else{[StringComparison]::Ordinal}))){throw 'KernelSdkBindingMismatch'}
}
$configuration = [Collections.Generic.List[string]]::new()
$configuration.Add('FORMAT=1')
$moduleCpuPath = Join-Path $contract 'Module/CpuBaseline.json'
$moduleCpu = Get-Content -LiteralPath $moduleCpuPath -Raw | ConvertFrom-Json
if ($moduleCpu.format -ne 1 -or $moduleCpu.cpu_model -cne 'x86_64' -or
    $moduleCpu.xcr0_mask -ne 7 -or $moduleCpu.name -cnotmatch '^[a-z0-9_-]{1,48}$' -or
    $moduleCpu.requirements.Count -lt 1 -or $moduleCpu.requirements.Count -gt 32) {
    throw 'Invalid canonical module CPU baseline; review its architecture and XSTATE contract.'
}
$configuration.Add('MODULE_CPU_NAME='+$moduleCpu.name)
$configuration.Add('MODULE_CPU_SHA256='+(Get-FileHash -LiteralPath $moduleCpuPath -Algorithm SHA256).Hash.ToLowerInvariant())
$configuration.Add('MODULE_CPU_XCR0_MASK='+[string]$moduleCpu.xcr0_mask)
$configuration.Add('MODULE_CPU_REQUIREMENT_COUNT='+[string]$moduleCpu.requirements.Count)
$cpuNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$cpuBits = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$cpuRegisters = @{eax=0;ebx=1;ecx=2;edx=3}
for ($cpuIndex=0; $cpuIndex -lt $moduleCpu.requirements.Count; $cpuIndex++) {
    $requirement = $moduleCpu.requirements[$cpuIndex]
    if ($requirement.name -cnotmatch '^[a-z0-9_]{1,32}$' -or
        -not $cpuRegisters.ContainsKey([string]$requirement.register) -or
        [uint32]$requirement.bit -ge 32 -or -not $cpuNames.Add([string]$requirement.name)) {
        throw 'Invalid or duplicate canonical CPU requirement.'
    }
    $cpuRow = [string][uint32]$requirement.leaf+','+[string][uint32]$requirement.subleaf+','+
        [string]$cpuRegisters[[string]$requirement.register]+','+[string][uint32]$requirement.bit
    if (-not $cpuBits.Add($cpuRow)) { throw 'Duplicate canonical CPUID requirement.' }
    $configuration.Add('MODULE_CPU_REQUIREMENT_'+[string]$cpuIndex+'='+$requirement.name+','+$cpuRow)
}
$flags = [uint64]0
foreach ($spec in $specs) {
    $number = $selected[$spec.name]
    $value = if ($spec.type -ceq 'bool') { if($number){'true'}else{'false'} } else { [string]$number }
    $configuration.Add($spec.config.ToUpperInvariant()+'='+$value)
    if ($spec.type -ceq 'bool' -and $number -ne 0) { $flags = $flags -bor ([uint64]1 -shl [int]$spec.bit) }
}
$platformSpec=Get-Content -Raw -LiteralPath (Join-Path $contract 'API/ApiContract.json') | ConvertFrom-Json
$platformGroups=@($platformSpec.groups | Where-Object kind -CEQ 'kernel_table')
$configuration.Add('PLATFORM_API_COUNT='+[string]$platformGroups.Count)
for($platformIndex=0;$platformIndex -lt $platformGroups.Count;$platformIndex++){
    $group=$platformGroups[$platformIndex]
    $configuration.Add('PLATFORM_API_'+[string]$platformIndex+'='+[string]$group.name+','+[string]$group.id)
}
$configuration.Add('FLAGS='+[string]$flags)
$bytes = [Text.Encoding]::UTF8.GetBytes(($configuration -join "`n")+"`n")
$configurationId = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
$rustOutput = Join-Path $artifacts 'Kernel/Rust'
$configurationRoot = Join-Path $rustOutput 'Configurations'
[void][IO.Directory]::CreateDirectory($configurationRoot)
$configurationPath = Join-Path $configurationRoot ($configurationId+'.R4S')
if (-not (Test-Path -LiteralPath $configurationPath)) { [IO.File]::WriteAllBytes($configurationPath,$bytes) }
if ((Get-FileHash -LiteralPath $configurationPath).Hash.ToLowerInvariant() -cne $configurationId) { throw 'Kernel build configuration was modified.' }
$rustFlags = @($codegen.rustflags)
$environment = @{
    R4OS_KERNEL_BUILD_CONFIG=$configurationPath
    R4OS_KERNEL_LINKER_FLAGS=($codegen.linker_flags -join [char]31)
    CARGO_ENCODED_RUSTFLAGS=($rustFlags -join [char]31)
}
foreach($key in $codegen.cargo_release.Keys){$environment['CARGO_PROFILE_RELEASE_'+$key]=[string]$codegen.cargo_release[$key]}
$saved = @{}
foreach ($key in $environment.Keys) { $saved[$key]=[Environment]::GetEnvironmentVariable($key,'Process'); Set-Item -LiteralPath ('Env:'+$key) -Value $environment[$key] }
try {
    if($command -ceq 'fmt') {
        & pwsh @runner cargo fmt --manifest-path $manifest -- --check
        if($LASTEXITCODE -ne 0){throw 'Kernel Rust formatting check failed.'}
        exit 0
    }
    $cargoArguments=@('cargo',$command,'--locked','--offline','--release','--manifest-path',$manifest,'--target',$codegen.target,'--target-dir',$rustOutput)
    $cargoArguments+=@($patches)
    if($command -ceq 'clippy'){$cargoArguments+=@('--','-D','warnings')}
    & pwsh @runner @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw ('Cargo Kernel '+$command+' failed.') }
    if($command -cne 'build'){exit 0}
    $binary=Join-Path $rustOutput ($codegen.target+'/release/r4os')
    if(-not(Test-Path -LiteralPath $binary -PathType Leaf)){throw 'Cargo did not produce the native Kernel ELF.'}
    # Preserve the established distribution artifact path on both hosts.
    $destination=Join-Path $kernel 'zig-out/bin'
    [void][IO.Directory]::CreateDirectory($destination)
    $elf=Join-Path $destination 'r4os.elf'
    Copy-Item -LiteralPath $binary -Destination $elf -Force
    if ($selected['strip-kernel'] -ne 0) {
        & pwsh @runner llvm-objcopy --strip-all $elf
        if ($LASTEXITCODE -ne 0) { throw 'Final Kernel ELF stripping failed.' }
    }
    $hash=(Get-FileHash -LiteralPath $elf).Hash.ToLowerInvariant()
    $version=([IO.File]::ReadAllText((Join-Path $kernel 'VERSION.R4S')).TrimStart([char]0xFEFF).Trim() -split '=',2)[1]
    $rustVersion=(@(& pwsh @runner rustc --version) -join ' ').Trim()
    if($LASTEXITCODE -ne 0){throw 'Rust version query failed.'}
    $metadata=[ordered]@{schema=1;sdk_profile=@{id=$context.resolved.profile;hashes=$context.resolved.hashes;technical=$context.resolved.technical};artifact='r4os.elf';sha256=$hash;kernel_version=$version;languages=@('rust','assembly');
        compilers=[ordered]@{rust=$rustVersion};configuration_sha256=$configurationId;options=$selected;
        profiles=[ordered]@{rust=@{profile='release';opt_level=[int]$codegen.cargo_release.OPT_LEVEL;
            overflow_checks=[Convert]::ToBoolean($codegen.cargo_release.OVERFLOW_CHECKS);debug_assertions=[Convert]::ToBoolean($codegen.cargo_release.DEBUG_ASSERTIONS);
            panic=$codegen.cargo_release.PANIC;lto=[Convert]::ToBoolean($codegen.cargo_release.LTO);
            red_zone=$codegen.red_zone;simd=$codegen.simd;code_model=$codegen.code_model;relocation_model=$codegen.relocation_model;
            target=$codegen.target;rustflags=$rustFlags;linker=$codegen.linker;linker_flags=@($codegen.linker_flags)}}}
    [IO.File]::WriteAllText((Join-Path $destination 'r4os.build.json'),($metadata|ConvertTo-Json -Depth 20)+"`n",[Text.UTF8Encoding]::new($false))
} finally {
    foreach($key in $saved.Keys) { if($null -eq $saved[$key]){Remove-Item -LiteralPath ('Env:'+$key) -ErrorAction SilentlyContinue}else{Set-Item -LiteralPath ('Env:'+$key) -Value $saved[$key]} }
}
