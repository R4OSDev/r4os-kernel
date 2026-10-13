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
$zig = Join-Path (Resolve-KernelPath $devkit $settings.ZIG_ROOT) ('zig'+$(if($IsWindows){'.exe'}else{''}))
$rust = Join-Path $devkit 'Setup/Rust.ps1'
if (-not (Test-Path -LiteralPath $zig -PathType Leaf)) { throw 'Pinned Zig is missing; initialize the DevKit.' }
if (-not (Test-Path -LiteralPath $rust -PathType Leaf)) { throw 'Pinned Rust starter is missing; initialize the DevKit.' }
$pin = Get-Content -LiteralPath (Join-Path $devkit 'Setup/RustToolchain.json') -Raw | ConvertFrom-Json
if ($pin.rust_version -cne '1.99.0' -or $pin.kernel_target -cne 'x86_64-unknown-none') { throw 'Review and update the Kernel Rust pin before changing its toolchain.' }
if (Test-Path -LiteralPath (Join-Path $contract 'build.zig.zon')) {
    $graphText=@(& pwsh -NoLogo -NoProfile -File $rust cargo metadata --locked --format-version 1 --manifest-path (Join-Path $kernel 'Cargo.toml')) -join "`n"
    if($LASTEXITCODE -ne 0){throw 'Cannot resolve the exact Cargo Contract pin.'}
    $graph=$graphText|ConvertFrom-Json
    $package=@($graph.packages|Where-Object name -CEQ 'r4os-contract')
    if($package.Count -ne 1){throw 'Kernel must resolve exactly one canonical Contract crate.'}
    $pinnedContract=[IO.Path]::GetFullPath((Join-Path (Split-Path $package[0].manifest_path -Parent) '../../..'))
    foreach($relative in @('API/ApiContract.json','Generated/SDK/Zig/abi.zig','Generated/SDK/Zig/abi_generated.zig',
        'Generated/Kernel/Zig/r4x_api_generated.zig','Generated/SDK/Rust/src/lib.rs')) {
        if((Get-FileHash -LiteralPath (Join-Path $contract $relative)).Hash -cne (Get-FileHash -LiteralPath (Join-Path $pinnedContract $relative)).Hash) {
            throw ('Local Zig and pinned Rust Contract differ: '+$relative+'. Publish and update both Kernel pins before building an ABI change.')
        }
    }
}
$specs = @(Get-Content -LiteralPath (Join-Path $kernel 'BuildOptions.json') -Raw | ConvertFrom-Json)
$selected = [ordered]@{}
foreach ($spec in $specs) { $selected[$spec.name] = $spec.default }
foreach ($argument in $BuildArguments) {
    if ($argument -notmatch '^-D([^=]+)=(.*)$') { continue }
    $name,$value = $Matches[1],$Matches[2]
    $spec = @($specs | Where-Object name -CEQ $name)
    if ($spec.Count -eq 0) { continue }
    if ($spec[0].type -ceq 'bool') {
        if ($value -cnotin @('true','false')) { throw ('Kernel Boolean needs true/false: '+$name) }
        $selected[$name] = if($value -ceq 'true'){1}else{0}
    } else {
        $number = [uint32]0
        if (-not [uint32]::TryParse($value,[ref]$number)) { throw ('Kernel u32 option is invalid: '+$name) }
        $selected[$name] = $number
    }
}
$configuration = [Collections.Generic.List[string]]::new()
$configuration.Add('FORMAT=1')
$flags = [uint64]0
$zigArguments = [Collections.Generic.List[string]]::new()
foreach ($argument in $BuildArguments) {
    if ($argument -match '^-D([^=]+)=' -and $selected.Contains($Matches[1])) { continue }
    if ($argument -match '^-Drust-archive=') { throw 'The Kernel builder owns the Rust archive; callers cannot substitute it.' }
    $zigArguments.Add($argument)
}
foreach ($spec in $specs) {
    $number = $selected[$spec.name]
    $value = if ($spec.type -ceq 'bool') { if($number){'true'}else{'false'} } else { [string]$number }
    $configuration.Add($spec.config.ToUpperInvariant()+'='+$value)
    $zigArguments.Add('-D'+$spec.name+'='+$value)
    if ($spec.type -ceq 'bool' -and $number -ne 0) { $flags = $flags -bor ([uint64]1 -shl [int]$spec.bit) }
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
$elf = Join-Path $kernel 'zig-out/bin/r4os.elf'
$oldHash = if(Test-Path -LiteralPath $elf){(Get-FileHash -LiteralPath $elf).Hash.ToLowerInvariant()}else{''}
try {
    & pwsh -NoLogo -NoProfile -File $rust cargo build --locked --release --manifest-path (Join-Path $kernel 'Cargo.toml') --target $pin.kernel_target --target-dir $rustOutput
    if ($LASTEXITCODE -ne 0) { throw 'Cargo Kernel build failed.' }
    $archive = Join-Path $rustOutput ($pin.kernel_target+'/release/libr4os_kernel.a')
    if (-not (Test-Path -LiteralPath $archive)) { throw 'Cargo did not produce a native Kernel archive.' }
    $zigArguments.Add('-Drust-archive='+$archive)
    if ($BuildArguments -contains 'test') {
        # Run the existing Zig owner fixtures against native Rust C exports.
        # The isolated host archive has no boot entry or platform execution.
        $bareFlags=$env:CARGO_ENCODED_RUSTFLAGS
        try {
            $env:CARGO_ENCODED_RUSTFLAGS=@('-Ccode-model=small','-Crelocation-model=static','-Coverflow-checks=yes','-Cdebug-assertions=yes','-Cpanic=abort','-Clto=off') -join [char]31
            $ownerOutput=Join-Path $artifacts 'Kernel/RustOwners'
            # Like Contract ABI tests, Linux executes the actual bare SysV
            # implementation. Windows needs native COFF for its host runner.
            $ownerTarget=if($IsLinux){$pin.kernel_target}else{'x86_64-pc-windows-msvc'}
            & pwsh -NoLogo -NoProfile -File $rust cargo build --locked --release --manifest-path (Join-Path $kernel 'Tests/RustOwners/Cargo.toml') --target $ownerTarget --target-dir $ownerOutput
            if ($LASTEXITCODE -ne 0) { throw 'Rust host owner fixture build failed.' }
            $ownerArchive=Join-Path $ownerOutput ($ownerTarget+'/release/'+$(if($IsWindows){'r4os_kernel_owner_fixture.lib'}else{'libr4os_kernel_owner_fixture.a'}))
            if (-not (Test-Path -LiteralPath $ownerArchive)) { throw 'Rust host owner archive missing.' }
            $zigArguments.Add('-Drust-owner-test-archive='+$ownerArchive)
        } finally { $env:CARGO_ENCODED_RUSTFLAGS=$bareFlags }
    }
    if (Test-Path -LiteralPath (Join-Path $contract 'build.zig.zon')) { $zigArguments.Add('--fork='+$contract) }
    Push-Location $kernel
    try { & $zig build @($zigArguments.ToArray()); $result=$LASTEXITCODE } finally { Pop-Location }
    if ($result -ne 0) { throw ('Native Kernel linking/test failed: '+$result) }
    if (Test-Path -LiteralPath $elf) {
        $hash=(Get-FileHash -LiteralPath $elf).Hash.ToLowerInvariant()
        $steps=@($BuildArguments | Where-Object {$_ -notmatch '^-'})
        $linked=($steps.Count -eq 0 -or $steps -contains 'install' -or $hash -cne $oldHash)
        if ($linked) {
            # Zig's source-level strip does not remove debug carried by native
            # Rust/core objects. Apply the existing final-ELF strip contract.
            if ($selected['strip-kernel'] -ne 0) {
                & pwsh -NoLogo -NoProfile -File $rust llvm-objcopy --strip-all $elf
                if ($LASTEXITCODE -ne 0) { throw 'Final Kernel ELF stripping failed.' }
                $hash=(Get-FileHash -LiteralPath $elf).Hash.ToLowerInvariant()
            }
            $version=([IO.File]::ReadAllText((Join-Path $kernel 'VERSION.R4S')).TrimStart([char]0xFEFF).Trim() -split '=',2)[1]
            $rustVersion=(@(& pwsh -NoLogo -NoProfile -File $rust rustc --version) -join ' ').Trim()
            if($LASTEXITCODE -ne 0){throw 'Rust version query failed.'}
            $zigVersion=(@(& $zig version) -join ' ').Trim()
            if($LASTEXITCODE -ne 0){throw 'Zig version query failed.'}
            $metadata=[ordered]@{schema=1; artifact='r4os.elf'; sha256=$hash; kernel_version=$version; languages=@('zig','rust');
                compilers=[ordered]@{zig=$zigVersion;rust=$rustVersion}; configuration_sha256=$configurationId; options=$selected;
                profiles=[ordered]@{zig=@{optimize='ReleaseSafe';red_zone=$false;simd=$false;code_model='kernel'};
                    rust=@{profile='release';opt_level=3;overflow_checks=$true;debug_assertions=$true;panic='abort';lto=$false;red_zone=$false;simd=$false;code_model='kernel';relocation_model='static';target=$pin.kernel_target;rustflags=$rustFlags}}}
            [IO.File]::WriteAllText((Join-Path $kernel 'zig-out/bin/r4os.build.json'),($metadata|ConvertTo-Json -Depth 20)+"`n",[Text.UTF8Encoding]::new($false))
        }
    }
} finally {
    foreach($key in $saved.Keys) { if($null -eq $saved[$key]){Remove-Item -LiteralPath ('Env:'+$key) -ErrorAction SilentlyContinue}else{Set-Item -LiteralPath ('Env:'+$key) -Value $saved[$key]} }
}
