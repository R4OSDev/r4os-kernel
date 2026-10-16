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
$rust = Join-Path $devkit 'Setup/Rust.ps1'
if (-not (Test-Path -LiteralPath $rust -PathType Leaf)) { throw 'Pinned Rust starter is missing; initialize the DevKit.' }
$pin = Get-Content -LiteralPath (Join-Path $devkit 'Setup/RustToolchain.json') -Raw | ConvertFrom-Json
if ($pin.rust_version -cne '1.99.0' -or $pin.kernel_target -cne 'x86_64-unknown-none') { throw 'Review the checked Kernel profile before changing its Rust toolchain.' }
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
$graphText=@(& pwsh -NoLogo -NoProfile -File $rust cargo metadata --locked --format-version 1 --manifest-path $manifest) -join "`n"
if($LASTEXITCODE -ne 0){throw 'Cannot resolve the exact Cargo Contract pin.'}
$graph=$graphText|ConvertFrom-Json
$package=@($graph.packages|Where-Object name -CEQ 'r4os-contract')
if($package.Count -ne 1){throw 'Kernel must resolve exactly one canonical Contract crate.'}
$pinnedContract=[IO.Path]::GetFullPath((Join-Path (Split-Path $package[0].manifest_path -Parent) '../../..'))
foreach($relative in @('API/ApiContract.json','Generated/SDK/Rust/Cargo.toml','Generated/SDK/Rust/src/lib.rs')) {
    if((Get-FileHash -LiteralPath (Join-Path $contract $relative)).Hash -cne (Get-FileHash -LiteralPath (Join-Path $pinnedContract $relative)).Hash) {
        throw ('Workspace and pinned Rust Contract differ: '+$relative+'. Publish and update the Kernel pin before building an ABI change.')
    }
}
$configuration = [Collections.Generic.List[string]]::new()
$configuration.Add('FORMAT=1')
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
$rustFlags = @('-Ccode-model=kernel','-Crelocation-model=static','-Cno-redzone=yes','-Coverflow-checks=yes',
    '-Cdebug-assertions=yes','-Cpanic=abort','-Copt-level=3','-Clto=off','-Cembed-bitcode=no')
$environment = @{
    R4OS_KERNEL_BUILD_CONFIG=$configurationPath
    CARGO_ENCODED_RUSTFLAGS=($rustFlags -join [char]31)
    CARGO_PROFILE_RELEASE_OPT_LEVEL='3'; CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS='true'
    CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS='true'; CARGO_PROFILE_RELEASE_PANIC='abort'; CARGO_PROFILE_RELEASE_LTO='false'
}
$saved = @{}
foreach ($key in $environment.Keys) { $saved[$key]=[Environment]::GetEnvironmentVariable($key,'Process'); Set-Item -LiteralPath ('Env:'+$key) -Value $environment[$key] }
try {
    if($command -ceq 'fmt') {
        & pwsh -NoLogo -NoProfile -File $rust cargo fmt --manifest-path $manifest -- --check
        if($LASTEXITCODE -ne 0){throw 'Kernel Rust formatting check failed.'}
        exit 0
    }
    $cargoArguments=@('cargo',$command,'--locked','--release','--manifest-path',$manifest,'--target',$pin.kernel_target,'--target-dir',$rustOutput)
    if($command -ceq 'clippy'){$cargoArguments+=@('--','-D','warnings')}
    & pwsh -NoLogo -NoProfile -File $rust @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw ('Cargo Kernel '+$command+' failed.') }
    if($command -cne 'build'){exit 0}
    $binary=Join-Path $rustOutput ($pin.kernel_target+'/release/r4os')
    if(-not(Test-Path -LiteralPath $binary -PathType Leaf)){throw 'Cargo did not produce the native Kernel ELF.'}
    # Preserve the established distribution artifact path on both hosts.
    $destination=Join-Path $kernel 'zig-out/bin'
    [void][IO.Directory]::CreateDirectory($destination)
    $elf=Join-Path $destination 'r4os.elf'
    Copy-Item -LiteralPath $binary -Destination $elf -Force
    if ($selected['strip-kernel'] -ne 0) {
        & pwsh -NoLogo -NoProfile -File $rust llvm-objcopy --strip-all $elf
        if ($LASTEXITCODE -ne 0) { throw 'Final Kernel ELF stripping failed.' }
    }
    $hash=(Get-FileHash -LiteralPath $elf).Hash.ToLowerInvariant()
    $version=([IO.File]::ReadAllText((Join-Path $kernel 'VERSION.R4S')).TrimStart([char]0xFEFF).Trim() -split '=',2)[1]
    $rustVersion=(@(& pwsh -NoLogo -NoProfile -File $rust rustc --version) -join ' ').Trim()
    if($LASTEXITCODE -ne 0){throw 'Rust version query failed.'}
    $metadata=[ordered]@{schema=1;artifact='r4os.elf';sha256=$hash;kernel_version=$version;languages=@('rust','assembly');
        compilers=[ordered]@{rust=$rustVersion};configuration_sha256=$configurationId;options=$selected;
        profiles=[ordered]@{rust=@{profile='release';opt_level=3;overflow_checks=$true;debug_assertions=$true;panic='abort';lto=$false;
            red_zone=$false;simd=$false;code_model='kernel';relocation_model='static';target=$pin.kernel_target;rustflags=$rustFlags;linker='rust-lld'}}}
    [IO.File]::WriteAllText((Join-Path $destination 'r4os.build.json'),($metadata|ConvertTo-Json -Depth 20)+"`n",[Text.UTF8Encoding]::new($false))
} finally {
    foreach($key in $saved.Keys) { if($null -eq $saved[$key]){Remove-Item -LiteralPath ('Env:'+$key) -ErrorAction SilentlyContinue}else{Set-Item -LiteralPath ('Env:'+$key) -Value $saved[$key]} }
}
