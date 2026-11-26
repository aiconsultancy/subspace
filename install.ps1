<#
Subspace installer for Windows. On macOS and Linux, use install.sh.

A fleet is one private Linux VM: Stargate, the rear admiral and the captains
(one Linux user and tmux session each), CLIProxyAPI, Ergo IRC and the
Subspace API. This script creates the VM with Multipass on Hyper-V (Windows
Pro, Enterprise or Education) or connects to yours over SSH, copies the
installer onto it, runs subspace-setup there, locks the VM down, and
connects the desktop app to it through an SSH tunnel.

Read INSTALL.md first. Run it without changing your execution policy:

  powershell -ExecutionPolicy Bypass -File .\install.ps1 [COMMAND] [OPTIONS] [FLEET OPTIONS]

Commands, options and fleet options are the same as install.sh:
  install (default) | irc-server | tunnel | providers [login P] | skills | desktop | ssh | status | audit | stop | start | destroy
  --local | --remote USER@HOST   --target NAME   --identity FILE   --ssh-port N
  --host-key SHA256:...   --ssh-proxy CMD   --cpus N --memory GIB --disk GIB   --install-deps
  --irc-server NAME   --api-target NAME   --yes   --background   -h | --help
--api-target NAME installs a further fleet without its own Subspace API and
registers it with the API of the fleet installed as --target NAME; later runs
reuse it. --host-key always checks the VM's key, and repins a rebuilt VM.
--database-url-file FILE runs the VM's Subspace API against an existing
PostgreSQL: FILE holds its postgresql://...?sslmode=verify-full URL and is
copied to the VM as root's (add --migrate to migrate it); --database-ca-file
FILE gives that server's CA certificate (PEM) when the VM does not trust it.
--irc-operator-password-file FILE, with --irc-operator NAME, gives a remote
IRC server's operator password, so setup registers the officers' certificate
accounts itself; it is removed from the VM after the run.
--irc-operator-account-password-file FILE, with --irc-operator-account NAME,
does the same for the IRC account the operator's connection signs in to first,
for a server that admits only signed-in connections (require-sasl).
--api-enrolment-token-file FILE, with --api-url URL (a Subspace API on another
host), gives the fleet's enrolment token; it too is removed from the VM after
the run.
Every other option goes to subspace-setup on the VM, for example
  --name "Example Fleet" --rear-admiral Atlas --captains 'Orion,Vega' --admin admiral --demo
Quote comma-separated lists, such as --toolchains 'node@24,pnpm@9.12.3'.
#>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$SubspaceHome = if ($env:SUBSPACE_HOME) { $env:SUBSPACE_HOME } else { Join-Path $HOME '.subspace' }
$PayloadDir = '/opt/subspace/bootstrap'
$SetupCmd = '/opt/subspace/bootstrap/subspace-installer/vm/setup.sh'

$FleetConsole = 'fleet-console'
$Command = 'install'; $Mode = ''; $Target = ''; $Remote = ''; $Identity = ''; $SshPort = 22; $HostKey = ''; $SshProxy = ''
$Cpus = 4; $Memory = 8; $Disk = 60; $InstallDeps = $false; $IrcTarget = ''; $ApiTarget = ''; $AssumeYes = $false
$Background = $false; $SkillsFailed = $false; $DatabaseUrlFile = ''; $DatabaseCaFile = ''; $IrcOperatorPasswordFile = ''; $IrcOperatorAccountPasswordFile = ''; $ApiEnrolmentTokenFile = ''; $Passthrough = New-Object System.Collections.Generic.List[string]

function Die([string]$Message) { Write-Host "install: $Message" -ForegroundColor Red; exit 1 }
function Say([string]$Message) { Write-Host "==> $Message" }
function Note([string]$Message) { Write-Host "    $Message" }
function Test-Interactive { -not $AssumeYes -and [Environment]::UserInteractive -and -not [Console]::IsInputRedirected }
function Confirm-Step([string]$Question) {
    if (-not (Test-Interactive)) { return $false }
    $answer = Read-Host "$Question [y/N]"
    return $answer -match '^(y|yes)$'
}

# Arguments ------------------------------------------------------------------

# PowerShell passes an unquoted comma-separated value (--toolchains node@24,pnpm@9.12.3) to a script as an array,
# which [string] would join with spaces; setup on the VM wants the commas back.
$list = @(foreach ($value in $args) { if ($value -is [array]) { ($value | ForEach-Object { [string]$_ }) -join ',' } else { $value } })
$i = 0
if ($list.Count -gt 0 -and $list[0] -in @('install', 'irc-server', 'tunnel', 'providers', 'skills', 'desktop', 'ssh', 'status', 'audit', 'stop', 'start', 'destroy')) {
    $Command = $list[0]; $i = 1
}
function Next-Value { $script:i++; if ($script:i -ge $list.Count) { Die "$($list[$script:i - 1]) needs a value." }; return [string]$list[$script:i] }
for (; $i -lt $list.Count; $i++) {
    $arg = [string]$list[$i]
    if ($Command -eq 'ssh') {
        # The remote command is not the installer's: from '--' or its first word, everything goes to ssh unchanged.
        if ($arg -eq '--' -or -not $arg.StartsWith('-')) { $Passthrough.AddRange([string[]]@($list)[$i..($list.Count - 1)]); break }
        if ($arg -cmatch '^-[BbcDEeFIiJLlmOoPpQRSWw]$') { $Passthrough.Add($arg); $Passthrough.Add([string](Next-Value)); continue }
    }
    switch -CaseSensitive ($arg) {
        '--local' { $Mode = 'local' }
        '--remote' { $Mode = 'remote'; $Remote = Next-Value }
        '--target' { $Target = Next-Value }
        '--identity' { $Identity = Next-Value }
        '--ssh-port' { $SshPort = [int](Next-Value) }
        '--host-key' { $HostKey = Next-Value }
        '--ssh-proxy' { $SshProxy = Next-Value }
        '--cpus' { $Cpus = [int](Next-Value) }
        '--memory' { $Memory = [int](Next-Value) }
        '--disk' { $Disk = [int](Next-Value) }
        '--install-deps' { $InstallDeps = $true }
        '--irc-server' { $IrcTarget = Next-Value }
        '--api-target' { $ApiTarget = Next-Value }
        '--database-url-file' { $DatabaseUrlFile = Next-Value }
        '--database-ca-file' { $DatabaseCaFile = Next-Value }
        '--irc-operator-password-file' { $IrcOperatorPasswordFile = Next-Value }
        '--irc-operator-account-password-file' { $IrcOperatorAccountPasswordFile = Next-Value }
        '--api-enrolment-token-file' { $ApiEnrolmentTokenFile = Next-Value }
        '--fleet-console' { $FleetConsole = Next-Value }
        { $_ -in @('--yes', '-y') } { $AssumeYes = $true }
        '--background' { $Background = $true }
        { $_ -in @('-h', '--help') } {
            $help = @(Get-Content $MyInvocation.MyCommand.Path)
            $help[1..([Array]::IndexOf($help, '#>') - 1)]
            exit 0
        }
        default { $Passthrough.Add($arg) }
    }
}
if (-not $Target) { $Target = if ($Command -eq 'irc-server') { 'subspace-irc' } else { 'subspace-fleet' } }
foreach ($name in @($Target, $IrcTarget, $ApiTarget)) {
    if ($name -and $name -cnotmatch '^[a-z0-9][a-z0-9-]*$') { Die 'Target names must be lower-case letters, digits and hyphens.' }
}
if ($HostKey -and $HostKey -cnotmatch '^SHA256:[A-Za-z0-9+/]{43}$') { Die '--host-key needs a fingerprint such as SHA256:AbC... (43 characters after SHA256:).' }
foreach ($value in $Passthrough) { if ($value.Contains('"')) { Die 'Values cannot contain double quotes.' } }
$State = Join-Path (Join-Path $SubspaceHome 'targets') $Target

if ($Command -ne 'skills' -and -not (Get-Command ssh.exe -ErrorAction SilentlyContinue)) {
    Die 'The OpenSSH client is missing. Add it in Settings > System > Optional features > OpenSSH Client, then rerun.'
}

# Helpers --------------------------------------------------------------------

function Protect-File([string]$Path) {
    # Owner-only access, as OpenSSH and the desktop expect for keys and passwords: a protected
    # ACL with one entry for the current account's SID ($env:USERNAME is wrong for SYSTEM), which
    # also drops entries that tools such as ssh-keygen add explicitly.
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl = New-Object Security.AccessControl.FileSecurity
    $acl.SetOwner($sid)
    $acl.SetAccessRuleProtection($true, $false)
    $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'Allow')))
    try { Set-Acl -LiteralPath $Path -AclObject $acl } catch { Die "Could not restrict access to ${Path}: $_" }
}

function Write-Private([string]$Path, [string]$Text) {
    # Restrict the file before anything is written to it; overwriting keeps that ACL.
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    if (-not (Test-Path -LiteralPath $Path)) { [IO.File]::WriteAllText($Path, '') }
    Protect-File $Path
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding $false))
}

function Read-Env([string]$Path) {
    $values = @{}
    if (Test-Path $Path) {
        foreach ($line in Get-Content $Path) {
            $key, $value = $line -split '=', 2
            if ($key -match '^[A-Z_]+$') { $values[$key] = $value }
        }
    }
    return $values
}

function Invoke-Quiet([scriptblock]$Command) {
    # Run a native command, returning its standard output and ignoring its stderr. In Windows
    # PowerShell 5.1, redirected native stderr is a terminating error under 'Stop'.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { return @(& $Command 2>$null) } finally { $ErrorActionPreference = $previous }
}

function Get-HostKeys([string]$HostName, [int]$Port, [string]$Proxy = '',
    [string]$Algorithms = 'ssh-ed25519,ecdsa-sha2-nistp256,rsa-sha2-512') {
    # Read a server's SSH host key with ssh.exe itself: Windows' ssh-keyscan.exe offers a key
    # exchange it cannot complete against current OpenSSH. Authentication 'none' means no key
    # of ours is offered before the host key has been checked. ssh records one key: the first
    # of $Algorithms that the server has.
    $tmp = Join-Path $State 'hostkeys.tmp'
    Remove-Item -Force $tmp -ErrorAction SilentlyContinue
    $sshArgs = @('-o', "UserKnownHostsFile=$(Format-SshPath $tmp)", '-o', 'StrictHostKeyChecking=accept-new',
        '-o', 'BatchMode=yes', '-o', 'PreferredAuthentications=none', '-o', 'PubkeyAuthentication=no',
        '-o', 'ConnectTimeout=20', '-o', "HostKeyAlgorithms=$Algorithms", '-p', "$Port")
    if ($Proxy) { $sshArgs += @('-o', "ProxyCommand=$Proxy") }
    Invoke-Quiet { & ssh.exe @sshArgs "hostkey-check@$HostName" exit } | Out-Null
    $lines = @(if (Test-Path $tmp) { Get-Content $tmp | Where-Object { $_ -and -not $_.StartsWith('#') } })
    Remove-Item -Force $tmp -ErrorAction SilentlyContinue
    return $lines
}

function Get-KeyFingerprint([string]$Line) {
    # The SHA256 fingerprint of a known_hosts line, as ssh-keygen -l prints it; '' for anything else.
    $fields = @($Line.Trim() -split '\s+')
    if ($fields.Count -lt 3 -or $fields[0].StartsWith('#') -or $fields[0].StartsWith('@')) { return '' }
    try { $blob = [Convert]::FromBase64String($fields[2]) } catch { return '' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($blob) } finally { $sha.Dispose() }
    return 'SHA256:' + [Convert]::ToBase64String($hash).TrimEnd('=')
}

function Select-MatchingKey([string[]]$Lines, [string]$Fingerprint) {
    # The known_hosts lines whose key has this fingerprint (compared case-sensitively, like base64).
    return @($Lines | Where-Object { $_ -and (Get-KeyFingerprint $_) -ceq $Fingerprint })
}

function Quote-Posix([string]$Value) { return "'" + $Value.Replace("'", "'\''") + "'" }

function Format-SshPath([string]$Path) { return '"' + ($Path -replace '\\', '/') + '"' }

function Get-TargetEnv {
    $file = Join-Path $State 'target.env'
    if (-not (Test-Path $file)) { Die "No target '$Target'. Run .\install.ps1 --local or --remote USER@HOST first." }
    return Read-Env $file
}

function Update-SshConfig {
    # Multipass VMs can change address on reboot; refresh it before every SSH use.
    $t = Get-TargetEnv
    if ($t.KIND -eq 'multipass') {
        $info = (& multipass info $t.VM --format json | ConvertFrom-Json)
        $ips = @($info.info.($t.VM).ipv4)
        $ip = if ($ips.Count) { $ips[0] } else { '' }
        if (-not $ip) { Die "Multipass VM $($t.VM) has no address. Is it running? (.\install.ps1 start)" }
        $lines = @(
            'Host subspace-target', "  HostName $ip", '  User ubuntu', "  HostKeyAlias subspace-$($t.VM)",
            "  IdentityFile $(Format-SshPath (Join-Path $State 'id_ed25519'))", '  IdentitiesOnly yes'
        )
    } else {
        $user, $hostName = $t.HOST -split '@', 2
        $lines = @('Host subspace-target', "  HostName $hostName", "  User $user", "  Port $($t.PORT)")
        if ($t.IDENTITY) { $lines += @("  IdentityFile $(Format-SshPath $t.IDENTITY)", '  IdentitiesOnly yes') }
        if ($t.PROXY) { $lines += "  ProxyCommand $($t.PROXY)" }
    }
    $lines += @(
        "  UserKnownHostsFile $(Format-SshPath (Join-Path $State 'known_hosts'))", '  GlobalKnownHostsFile none',
        '  StrictHostKeyChecking yes',
        '  ForwardAgent no', '  ForwardX11 no', '  ServerAliveInterval 30', '  ServerAliveCountMax 3'
    )
    Write-Private (Join-Path $State 'ssh_config') (($lines -join "`n") + "`n")
}

function Invoke-Vm([string[]]$Rest, [switch]$Tty, [string]$Forward = '') {
    # Run a command on the VM attached to this console (so prompts work) and return its exit code.
    $sshArgs = @('-F', (Join-Path $State 'ssh_config'))
    if ($Forward) { $sshArgs += @('-o', 'ExitOnForwardFailure=yes', '-L', "${Forward}:127.0.0.1:$Forward") }
    if ($Tty -and (Test-Interactive)) { $sshArgs += '-t' } else { $sshArgs += @('-o', 'BatchMode=yes') }
    $sshArgs += 'subspace-target'
    $sshArgs += $Rest
    $process = Start-Process -FilePath 'ssh.exe' -ArgumentList (Format-Arguments $sshArgs) -NoNewWindow -Wait -PassThru
    return $process.ExitCode
}

function Format-Arguments([string[]]$Values) {
    # Windows command-line quoting for Start-Process. Values never contain double quotes.
    return $Values | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
}

function Read-Vm([string[]]$Rest) {
    # Run a command on the VM and capture its standard output and exit code; stderr stays on the console.
    $output = & ssh.exe -F (Join-Path $State 'ssh_config') -o BatchMode=yes subspace-target @Rest
    return [pscustomobject]@{ Code = $LASTEXITCODE; Text = (@($output) -join "`n") }
}

function Get-VmOutput([string[]]$Rest) {
    $result = Read-Vm $Rest
    if ($result.Code -ne 0) { Die "Remote command failed: $($Rest -join ' ')" }
    return $result.Text
}

function Send-VmInput([string]$InputFile, [string[]]$Rest) {
    # Run a command on the VM with a local file as its standard input and return its exit code.
    # Start-Process hands ssh.exe the file itself, so binary data arrives intact, and nothing is
    # staged in the VM's /tmp, where other users could read or replace it.
    $sshArgs = @('-F', (Join-Path $State 'ssh_config'), '-o', 'BatchMode=yes', 'subspace-target') + $Rest
    $process = Start-Process -FilePath 'ssh.exe' -ArgumentList (Format-Arguments $sshArgs) `
        -RedirectStandardInput $InputFile -NoNewWindow -Wait -PassThru
    return $process.ExitCode
}

function Invoke-OnTarget([string]$Name, [scriptblock]$Body) {
    # Run $Body against another saved target, such as the shared IRC server, then switch back.
    $saved = $script:State
    $script:State = Join-Path (Join-Path $SubspaceHome 'targets') $Name
    try {
        Update-SshConfig
        & $Body
    } finally { $script:State = $saved }
}

function Save-Private([string]$Path, [string[]]$Rest) {
    Write-Private $Path ((Get-VmOutput $Rest) + "`n")
}

# Local VM (Multipass) -------------------------------------------------------

function Install-Multipass {
    if (Get-Command multipass -ErrorAction SilentlyContinue) { return }
    $default = Join-Path $env:ProgramFiles 'Multipass\bin'
    if (Test-Path (Join-Path $default 'multipass.exe')) { $env:PATH = "$default;$env:PATH"; return }
    Say 'Multipass is not installed. It runs Ubuntu VMs on Hyper-V, which needs Windows Pro, Enterprise or Education.'
    Note 'On Windows Home, use an existing VM with --remote instead.'
    Note 'Project: https://canonical.com/multipass (GPL-3.0, by Canonical).'
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Die 'Install Multipass from https://canonical.com/multipass/install, then rerun.'
    }
    if (-not ($InstallDeps -or (Confirm-Step 'Install Multipass with winget (accepts its licence)?'))) {
        Die 'Multipass is required for a local VM. Install it, or rerun with --install-deps.'
    }
    & winget install --id Canonical.Multipass --exact --accept-package-agreements --accept-source-agreements
    $env:PATH = "$default;$env:PATH"
    if (-not (Get-Command multipass -ErrorAction SilentlyContinue)) {
        Die 'Multipass installed. Restart PowerShell (and Windows if it asks you to enable Hyper-V), then rerun.'
    }
}

function Get-MultipassHostKey([string]$Name) {
    # 'multipass exec' waits for stdin to close; without a console it would hang, so give it an
    # empty file and a time limit.
    $empty = Join-Path $State 'empty.in'
    $out = Join-Path $State 'hostkey.out'
    [IO.File]::WriteAllText($empty, '')
    $process = Start-Process -FilePath 'multipass' -NoNewWindow -PassThru -RedirectStandardInput $empty `
        -RedirectStandardOutput $out -ArgumentList @('exec', $Name, '--', 'ssh-keygen', '-lf', '/etc/ssh/ssh_host_ed25519_key.pub')
    if (-not $process.WaitForExit(60000)) { $process.Kill(); Die 'Multipass did not answer within a minute.' }
    $line = (Get-Content $out -ErrorAction SilentlyContinue | Select-Object -First 1)
    Remove-Item -Force $empty, $out -ErrorAction SilentlyContinue
    return $line
}

function New-MultipassVm {
    Install-Multipass
    New-Item -ItemType Directory -Force -Path $State | Out-Null
    $key = Join-Path $State 'id_ed25519'
    if (-not (Test-Path $key)) {
        # Start-Process keeps the empty passphrase argument intact on every PowerShell version.
        $keygen = Start-Process -FilePath 'ssh-keygen.exe' -NoNewWindow -Wait -PassThru `
            -ArgumentList "-q -t ed25519 -N `"`" -C subspace-$Target -f `"$key`""
        if ($keygen.ExitCode -ne 0) { Die 'Could not create an SSH key.' }
        Protect-File $key
    }
    $exists = $true
    $probe = (Invoke-Quiet { & multipass list --format csv }) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        $why = (Invoke-Quiet { & multipass version 2>&1 }) -join ' '
        if ($why -match 'not authenticated') {
            Die ('Multipass does not trust this Windows account yet. Use the account that installed Multipass, or ' +
                'have it run "multipass set local.passphrase", then run "multipass authenticate" here.')
        }
        Die "Multipass is not responding: $why"
    }
    # Multipass VMs on VirtualBox sit behind NAT, so their address cannot be reached from Windows.
    $driver = (@(Invoke-Quiet { & multipass get local.driver }) -join '').Trim()
    if ($driver -eq 'virtualbox') {
        Die ('Multipass is using VirtualBox, whose VMs this installer cannot reach over SSH. Use Hyper-V ' +
            '(Windows Pro, Enterprise or Education) or an existing VM with --remote.')
    }
    if ($probe -notmatch "(?m)^$([regex]::Escape($Target)),") { $exists = $false }
    if (-not $exists) {
        $public = (Get-Content "$key.pub" -Raw).Trim()
        $cloudInit = Join-Path $State 'cloud-init.yaml'
        Write-Private $cloudInit ("#cloud-config`nssh_pwauth: false`nssh_authorized_keys:`n  - $public`n")
        Say "Creating local VM '$Target' (Ubuntu 24.04, $Cpus CPUs, $Memory GiB memory, $Disk GiB disk)."
        Note 'No Windows folders are shared with it.'
        & multipass launch 24.04 --name $Target --cpus $Cpus --memory "${Memory}G" --disk "${Disk}G" --cloud-init $cloudInit
        if ($LASTEXITCODE -ne 0) { Die 'multipass launch failed.' }
    } else {
        & multipass start $Target
    }
    Write-Private (Join-Path $State 'target.env') "KIND=multipass`nVM=$Target`n"
    Update-SshConfig
    Confirm-MultipassHostKey
}

function Confirm-MultipassHostKey {
    # Pin the local VM's host key once, or again when --host-key names a key the saved file lacks.
    $vmName = (Get-TargetEnv).VM
    $known = Join-Path $State 'known_hosts'
    if (Test-Path $known) {
        if (-not $HostKey) { return }
        $matching = @(Select-MatchingKey @(Get-Content $known) $HostKey)
        if ($matching.Count) { Write-Private $known (($matching -join "`n") + "`n"); return }
        Say "The saved host key for '$vmName' is not $HostKey; reading it again."
    }
    # Read the VM's host key through Multipass itself, a channel that does not depend on the network.
    $expected = (Get-MultipassHostKey $vmName) -split ' ' | Select-Object -Index 1
    if (-not $expected) { Die 'Could not read the VM host key through Multipass.' }
    if ($HostKey -and $expected -cne $HostKey) { Die "The VM's host key is $expected, not $HostKey. Do not continue until you know why." }
    $ip = ((Get-Content (Join-Path $State 'ssh_config')) -match 'HostName' -split ' ')[-1]
    $lines = @(Select-MatchingKey @(Get-HostKeys $ip 22 '' 'ssh-ed25519') $expected)
    if (-not $lines.Count) { Die 'The VM host key seen over SSH does not match the one Multipass reports.' }
    Write-Private $known (($lines[0] -replace '^\S+', "subspace-$vmName") + "`n")
}

# Remote VM (SSH) ------------------------------------------------------------

function Connect-Remote {
    if ($Remote -notmatch '^[A-Za-z0-9._-]+@[A-Za-z0-9._:%-]+$') { Die '--remote needs USER@HOST.' }
    if ($Identity -and -not (Test-Path $Identity)) { Die "No SSH key at $Identity." }
    New-Item -ItemType Directory -Force -Path $State | Out-Null
    Write-Private (Join-Path $State 'target.env') "KIND=ssh`nHOST=$Remote`nPORT=$SshPort`nIDENTITY=$Identity`nPROXY=$SshProxy`n"
    Update-SshConfig
    Confirm-RemoteHostKey
    if ((Invoke-Vm @('true')) -ne 0) { Die "SSH to $Remote failed. Check the user, key and that the VM allows your address." }
    if ((Invoke-Vm @('sudo', '-n', 'true')) -ne 0) { Die 'The SSH user needs passwordless sudo on the VM for setup.' }
}

function Confirm-RemoteHostKey {
    # Saved keys are trusted unless --host-key names one they lack: then scan and repin, which is
    # how a rebuilt VM is accepted. Only keys with that fingerprint are ever written.
    $t = Get-TargetEnv
    $hostName = ($t.HOST -split '@', 2)[1]
    $port = [int]$t.PORT
    $known = Join-Path $State 'known_hosts'
    if (Test-Path $known) {
        if (-not $HostKey) { return }
        $matching = @(Select-MatchingKey @(Get-Content $known) $HostKey)
        if ($matching.Count) { Write-Private $known (($matching -join "`n") + "`n"); return }
        Say "The saved host key for $hostName is not $HostKey; checking the VM again."
    }
    Say "Checking $hostName's SSH host key."
    $scan = @(Get-HostKeys $hostName $port $t.PROXY)
    if ($scan.Count -eq 0) { Die "Could not reach SSH on ${hostName}:$port." }
    if ($HostKey) {
        $lines = @(Select-MatchingKey $scan $HostKey)
        # ssh records one key per connection, so look for the other key types too.
        foreach ($algorithm in @('ecdsa-sha2-nistp256', 'rsa-sha2-512,rsa-sha2-256')) {
            if (-not $lines.Count) { $lines = @(Select-MatchingKey @(Get-HostKeys $hostName $port $t.PROXY $algorithm) $HostKey) }
        }
        if (-not $lines.Count) { Die "No host key on $hostName matches $HostKey. Do not continue until you know why." }
    } else {
        foreach ($line in $scan) { Note "$(Get-KeyFingerprint $line) ($(@($line -split '\s+')[1]))" }
        Note "Compare these with the VM's own fingerprints (cloud console boot log, or"
        Note "'ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub' through your cloud's run-command)."
        if (-not (Confirm-Step 'Do they match?')) { Die 'Rerun with --host-key SHA256:... once you have the real fingerprint.' }
        $lines = $scan
    }
    Write-Private $known (($lines -join "`n") + "`n")
}

function Connect-Target {
    # Create or connect the VM for this command, or reuse the saved one.
    switch ($Mode) {
        'local' { New-MultipassVm }
        'remote' { Connect-Remote }
        default {
            Update-SshConfig
            if ((Get-TargetEnv).KIND -eq 'multipass') { Confirm-MultipassHostKey } else { Confirm-RemoteHostKey }
        }
    }
}

# Payload, desktop and tunnel ------------------------------------------------

function Send-Payload([string]$PayloadDir = $script:PayloadDir) {
    # The installer goes to /opt/subspace/bootstrap, or a dry run's own directory.
    Say "Copying the installer to $PayloadDir on the VM."
    $tarball = Join-Path ([IO.Path]::GetTempPath()) "subspace-payload-$PID.tgz"
    try {
        Push-Location $Root
        try {
            $paths = @('subspace-installer', 'subspace-api', 'subspace-irc', 'scripts/local-stack')
            $inGit = $false
            if (Get-Command git -ErrorAction SilentlyContinue) {
                $inGit = (Invoke-Quiet { & git rev-parse --is-inside-work-tree }) -contains 'true'
            }
            if ($inGit) {
                $listFile = Join-Path ([IO.Path]::GetTempPath()) "subspace-files-$PID.txt"
                & git ls-files --cached --others --exclude-standard -- @paths | Set-Content -Encoding ascii $listFile
                & tar.exe -czf $tarball -T $listFile
                Remove-Item $listFile
            } else {
                & tar.exe -czf $tarball --exclude node_modules --exclude .data --exclude .env @paths
            }
            if ($LASTEXITCODE -ne 0) { Die 'Could not package the installer.' }
        } finally { Pop-Location }
        # The archive goes over SSH's standard input straight into tar, so it never rests in the VM's /tmp.
        $script = "rm -rf $PayloadDir.new && mkdir -p $PayloadDir.new && tar --warning=no-unknown-keyword --no-same-owner -xzf - -C $PayloadDir.new && chown -R root:root $PayloadDir.new && chmod -R go-w $PayloadDir.new && find $PayloadDir.new -name '*.sh' -exec chmod 755 {} + && chmod 755 $PayloadDir.new/subspace-installer/vm/*.py $PayloadDir.new/subspace-installer/lib/*.py && rm -rf $PayloadDir && mv $PayloadDir.new $PayloadDir"
        if ((Send-VmInput $tarball @('sudo', '-n', 'sh', '-c', (Quote-Posix $script))) -ne 0) { Die 'Could not copy the installer to the VM and unpack it.' }
    } finally { Remove-Item -Force $tarball -ErrorAction SilentlyContinue }
}

function Confirm-Lockdown {
    # Setup only counts as done once the lockdown is confirmed and the privacy audit passes.
    $off = (Read-Env (Join-Path $State 'summary.env'))['SUBSPACE_LOCKDOWN'] -eq '0'
    if ($off) {
        Write-Warning 'Setup ran with --no-lockdown, so this VM has no lockdown firewall or SSH hardening.'
        $audit = Read-Vm @('sudo', '-n', 'subspace-setup', 'audit')
        $audit.Text -split "`n" | Where-Object { $_ -match '^\s*FAIL' } | ForEach-Object { Write-Warning $_ }
        return
    }
    $expected = (Read-Env (Join-Path $State 'summary.env'))['SUBSPACE_LOCKDOWN_ID']
    $status = Read-Vm @('sudo', '-n', 'subspace-setup', 'lockdown', 'status')
    if ($status.Text -match 'PENDING') {
        # Every ssh.exe run is a new connection (no connection sharing), which proves new logins still work.
        Say 'Confirming the lockdown over a new SSH connection.'
        $confirmArgs = @('sudo', '-n', 'subspace-setup', 'lockdown', 'confirm')
        if ($expected) { $confirmArgs += $expected }
        if ((Invoke-Vm $confirmArgs) -ne 0) {
            Note 'A new SSH connection failed, so the lockdown rolls back automatically within 3 minutes.'
            Note "Check the VM's firewall and SSH settings (INSTALL.md, 'If you get locked out'), then rerun."
            Die 'The lockdown could not be confirmed.'
        }
        $status = Read-Vm @('sudo', '-n', 'subspace-setup', 'lockdown', 'status')
    }
    $lockedDown = $status.Code -eq 0 -and $status.Text -notmatch 'PENDING' -and $status.Text -match 'firewall: loaded' -and
        $status.Text -match 'boot: confirmed'
    if (-not $lockedDown) {
        $status.Text -split "`n" | ForEach-Object { Note $_ }
        Die 'The lockdown is not in place on the VM. Read its status above, then run the installer again.'
    }
    if ($expected -and -not $status.Text.Contains("(ID $expected)")) {
        Die "The lockdown change $expected rolled back before it was confirmed. Run the installer again."
    }
    $audit = Read-Vm @('sudo', '-n', 'subspace-setup', 'audit')
    if ($audit.Code -ne 0) {
        $audit.Text -split "`n" | Where-Object { $_ -match '^\s*FAIL' } | ForEach-Object { Write-Warning $_ }
        Die 'Setup finished, but the privacy audit failed. Fix the FAIL lines, then run the installer again.'
    }
    Say 'The lockdown is confirmed and the privacy audit passed.'
}

function Get-DesktopDir {
    if ($env:SUBSPACE_DESKTOP_DIR) { return $env:SUBSPACE_DESKTOP_DIR }
    if (Test-Path (Join-Path $HOME '.config\subspace\config.json')) { return (Join-Path $HOME '.config\subspace') }
    return (Join-Path $env:APPDATA 'subspace')
}

function Write-DesktopConfig([string]$IrcName) {
    $s = Read-Env (Join-Path $State 'summary.env')
    if (-not $s.SUBSPACE_FLEET_SLUG) { Die 'No fleet summary yet; run the install first.' }
    if (-not $IrcName -and (Test-Path (Join-Path $State 'irc-target'))) { $IrcName = (Get-Content (Join-Path $State 'irc-target') -Raw).Trim() }
    if ($IrcName) {
        $ircSummary = Read-Env (Join-Path (Join-Path (Join-Path $SubspaceHome 'targets') $IrcName) 'summary.env')
        if ($ircSummary['SUBSPACE_IRC_HOST'] -ne $s.SUBSPACE_IRC_HOST) {
            # The fleet moved to another IRC server: the saved password and CA no longer apply.
            $IrcName = ''
            Remove-Item -Force (Join-Path $State 'irc-target') -ErrorAction SilentlyContinue
        }
    }
    if ($s.SUBSPACE_IRC_MODE -eq 'remote' -and -not $IrcName) {
        # Shared IRC that this installer does not manage (--irc remote --irc-host): no saved password or CA.
        $port = if ($s.SUBSPACE_IRC_PORT) { $s.SUBSPACE_IRC_PORT } else { '6697' }
        Say "This fleet uses the IRC server $($s.SUBSPACE_IRC_HOST), which this installer does not manage."
        Note "Add it in the desktop under Settings > Channels: server $($s.SUBSPACE_IRC_HOST), port $port, TLS on,"
        Note "and your IRC account and the password from that server's operator."
    } else {
        Write-IrcNetwork $s $IrcName
    }
    Write-FleetConsole $s
}

function Write-IrcNetwork($s, [string]$IrcName) {
    $channels = @($s.SUBSPACE_CHANNELS -split ' ' | Where-Object { $_ })
    if ($s.SUBSPACE_IRC_MODE -eq 'remote') {
        $ircState = Join-Path (Join-Path $SubspaceHome 'targets') $IrcName
        $entry = [ordered]@{ id = $IrcName; name = "Shared IRC ($($s.SUBSPACE_IRC_HOST))"; host = $s.SUBSPACE_IRC_HOST
            port = [int]$s.SUBSPACE_IRC_PORT; tls = $true }
        $ca = Join-Path $ircState 'ca.pem'
        if (Test-Path $ca) { $entry.caCert = $ca }
        $password = Join-Path $ircState "$($s.SUBSPACE_ADMIN_ACCOUNT).password"
    } else {
        $entry = [ordered]@{ id = $s.SUBSPACE_FLEET_SLUG; name = "$($s.SUBSPACE_FLEET_NAME) (fleet VM)"; host = '127.0.0.1'
            port = 6667; tls = $false }
        $password = Join-Path $State 'irc.password'
    }
    $entry.nickname = $s.SUBSPACE_ADMIN_ACCOUNT
    $entry.sasl = [ordered]@{ username = $s.SUBSPACE_ADMIN_ACCOUNT; passwordFile = $password }
    $entry.channels = $channels
    $entry.autoConnect = $true
    $dir = Join-Path (Get-DesktopDir) 'irc'
    $file = Join-Path $dir 'config.json'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $config = if (Test-Path $file) { Get-Content $file -Raw | ConvertFrom-Json } else { [pscustomobject]@{ networks = @() } }
    if (-not ($config.PSObject.Properties.Name -contains 'networks')) { $config | Add-Member networks @() }
    $config.networks = @(@($config.networks | Where-Object { $_.id -ne $entry.id }) + [pscustomobject]$entry)
    Write-Private $file ($config | ConvertTo-Json -Depth 8)
    Say "Wrote the desktop's IRC connection to $file."
}

function Write-FleetConsole($s) {
    # Point the Fleet Console web Space at this fleet's Stargate API through the tunnel.
    $tokenFile = Join-Path $State 'stargate-api.token'
    if ($s.SUBSPACE_STARGATE_API -ne '1' -or -not (Test-Path $tokenFile)) { return }
    $port = if ($s.SUBSPACE_STARGATE_API_PORT) { $s.SUBSPACE_STARGATE_API_PORT } else { '8484' }
    $endpoint = "http://127.0.0.1:$port"
    $file = Join-Path (Get-DesktopDir) 'config.json'
    $config = if (Test-Path $file) { Get-Content $file -Raw | ConvertFrom-Json } else { $null }
    $board = $null
    if ($config -and ($config.PSObject.Properties.Name -contains 'webBoards') -and
        ($config.webBoards.PSObject.Properties.Name -contains $FleetConsole)) { $board = $config.webBoards.$FleetConsole }
    if (-not $board) {
        Say "Fleet Console: install its web Space with Board ID '$FleetConsole' (Settings > Application > Web Spaces),"
        Note "quit the app and run '.\install.ps1 desktop' again; or enter the address $endpoint and the token"
        Note "from $tokenFile in Settings > Spaces > Fleet Console."
        return
    }
    foreach ($key in @('settings', 'secrets')) {
        if (-not ($board.PSObject.Properties.Name -contains $key)) { $board | Add-Member $key ([pscustomobject]@{}) }
    }
    $board.settings | Add-Member -Force endpoint $endpoint
    $board.secrets | Add-Member -Force token ((Get-Content $tokenFile -Raw).Trim())
    Write-Private $file ($config | ConvertTo-Json -Depth 12)
    Say "Set the Fleet Console's Stargate API address ($endpoint) and token in $file."
}

function Start-Tunnel {
    Get-TargetEnv | Out-Null
    Update-SshConfig
    $s = Read-Env (Join-Path $State 'summary.env')
    $forwards = @()
    $busy = @()
    foreach ($port in ($s.SUBSPACE_PORTS -split ' ' | Where-Object { $_ })) {
        $client = New-Object Net.Sockets.TcpClient
        try { $client.Connect('127.0.0.1', [int]$port); $busy += $port }
        catch { Write-Verbose "Port $port is free." }
        finally { $client.Dispose() }
        $forwards += @('-L', "127.0.0.1:${port}:127.0.0.1:$port")
    }
    if ($busy.Count) { Die "Ports already in use here: $($busy -join ' '). Stop whatever uses them first." }
    $sshArgs = @('-N', '-o', 'ExitOnForwardFailure=yes', '-o', 'ServerAliveInterval=30', '-F', (Join-Path $State 'ssh_config')) + $forwards + @('subspace-target')
    Say "Forwarding 127.0.0.1 ports $($s.SUBSPACE_PORTS) to the fleet VM."
    Note 'API http://127.0.0.1:3000  IRC 127.0.0.1:6667  dashboard http://127.0.0.1:8070  AgentsView http://127.0.0.1:8080'
    if ($Background) {
        $process = Start-Process ssh.exe -ArgumentList (Format-Arguments $sshArgs) -WindowStyle Hidden -PassThru
        Write-Private (Join-Path $State 'tunnel.pid') "$($process.Id)`n"
        Note "Tunnel running as process $($process.Id). Stop it with: Stop-Process -Id $($process.Id)"
    } else {
        & ssh.exe @sshArgs
    }
}

# Commands -------------------------------------------------------------------

function Invoke-Install {
    # A further fleet shares the first fleet's Subspace API instead of running its own.
    $apiFile = Join-Path $State 'api-target'
    # A Subspace API on another host (--api-url, passed to setup) replaces another fleet's API, so a saved
    # --api-target from an earlier run is set aside, and forgotten once a real run succeeds.
    $hosted = [bool]($Passthrough | Where-Object { $_ -ceq '--api-url' -or $_ -clike '--api-url=*' })
    if (-not $ApiTarget -and (Test-Path $apiFile)) {
        if ($hosted) { Write-Host "    Leaving out the saved --api-target $((Get-Content $apiFile -Raw).Trim()): --api-url names this fleet's API now." }
        else { $script:ApiTarget = (Get-Content $apiFile -Raw).Trim() }
    }
    if ($ApiTarget) {
        if ($ApiTarget -ceq $Target) { Die '--api-target must name another fleet, the one whose Subspace API this fleet shares.' }
        $api = Read-Env (Join-Path (Join-Path (Join-Path $SubspaceHome 'targets') $ApiTarget) 'summary.env')
        if ($api['SUBSPACE_API'] -ne '1') { Die "No fleet '$ApiTarget' with a Subspace API. Install that fleet first, without --no-api." }
        Invoke-OnTarget $ApiTarget {
            if ((Invoke-Vm @('sudo', '-n', 'subspace-setup', 'register-fleet', '--check')) -ne 0) {
                Die "The Subspace API on '$ApiTarget' cannot register this fleet; nothing was changed."
            }
        }
        $Passthrough.Add('--shared-api-target')
    }
    if (-not $Mode) {
        if (Test-Path (Join-Path $State 'target.env')) {
            $script:Mode = if ((Get-TargetEnv).KIND -eq 'multipass') { 'local' } else { 'reuse' }
        } elseif (Test-Interactive) {
            Write-Host "Where should the fleet run?`n  1) a new local VM on this PC`n  2) an existing VM over SSH (for example in the cloud)"
            switch (Read-Host 'Choose 1 or 2') {
                '1' { $script:Mode = 'local' }
                '2' { $script:Mode = 'remote'; $script:Remote = Read-Host 'SSH login (USER@HOST)' }
                default { Die 'Choose 1 or 2.' }
            }
        } else { Die 'Choose --local or --remote USER@HOST.' }
    }
    Connect-Target
    if ((Invoke-Vm @('true')) -ne 0) { Die 'Cannot reach the VM over SSH.' }
    # The fleet's record in the Subspace API names the VM by the host this computer reaches it at.
    $targetEnv = Get-TargetEnv
    if ($targetEnv.KIND -eq 'ssh') { $Passthrough.AddRange([string[]]@('--vm-host', ($targetEnv.HOST -split '@', 2)[-1])) }

    # subspace-setup --dry-run changes nothing on the VM: the files it is given go to a scratch directory that
    # is removed afterwards, never over the inputs (such as the database URL) that the next real run uses.
    $dryRun = $Passthrough -contains '--dry-run'
    $inputs = if ($dryRun) { '/root/subspace-setup-dry-run' } else { '/root/fleet' }
    # Directories on the VM that hold this run's own copies (a dry run's files, the IRC operator's passwords),
    # removed when the run ends, however it ends.
    $transient = New-Object System.Collections.Generic.List[string]
    if ($dryRun) { $transient.Add($inputs) }
    # A Subspace API on another host (--api-url, passed to setup) excludes another fleet's API and a database here.
    if ($hosted) {
        if ($ApiTarget) { Die "--api-url names the fleet's API, so --api-target does not apply." }
        if ($DatabaseUrlFile) { Die '--api-url uses an API on another host, which keeps its own database: leave out --database-url-file.' }
    } elseif ($ApiEnrolmentTokenFile) { Die '--api-enrolment-token-file goes with --api-url.' }
    try {
        if ($IrcTarget) {
            $ircState = Join-Path (Join-Path $SubspaceHome 'targets') $IrcTarget
            $irc = Read-Env (Join-Path $ircState 'summary.env')
            if (-not $irc.SUBSPACE_IRC_HOST) { Die "No shared IRC server '$IrcTarget'. Run .\install.ps1 irc-server first." }
            $Passthrough.AddRange([string[]]@('--irc', 'remote', '--irc-host', $irc.SUBSPACE_IRC_HOST, '--irc-port', $irc.SUBSPACE_IRC_PORT))
            if ($irc.SUBSPACE_IRC_CERTIFICATE_SEEDS -eq '1') {
                $Passthrough.Add('--managed-shared-irc')
            } else {
                $Passthrough.AddRange([string[]]@('--irc-auth', 'password'))
            }
            if ($irc.SUBSPACE_IRC_SELF_SIGNED -eq '1') {
                $caSource = Join-Path $ircState 'ca.pem'
                $caDigest = (Get-FileHash -LiteralPath $caSource -Algorithm SHA256).Hash.ToLowerInvariant()
                $caTarget = "$inputs/irc-ca-$caDigest.pem"
                $caScript = "umask 077 && mkdir -p $inputs && cat > $caTarget.new && mv -f $caTarget.new $caTarget"
                if ((Send-VmInput $caSource @('sudo', '-n', 'sh', '-c', (Quote-Posix $caScript))) -ne 0) { Die 'Could not copy the IRC certificate to the VM.' }
                $Passthrough.AddRange([string[]]@('--irc-ca', $caTarget))
            } else {
                $Passthrough.Add('--irc-ca=')
            }
        }

        if ($DatabaseUrlFile) {
            # The URL holds the database's password: it travels over SSH on standard input, never in a command line.
            if (-not (Test-Path -LiteralPath $DatabaseUrlFile -PathType Leaf)) { Die "Cannot read --database-url-file $DatabaseUrlFile." }
            if ($ApiTarget) { Die "--database-url-file runs this VM's own Subspace API, which --api-target leaves out." }
            $urlScript = "umask 077 && mkdir -p $inputs && cat > $inputs/database-url.new && mv -f $inputs/database-url.new $inputs/database-url"
            if ((Send-VmInput (Resolve-Path -LiteralPath $DatabaseUrlFile).Path @('sudo', '-n', 'sh', '-c', (Quote-Posix $urlScript))) -ne 0) { Die 'Could not copy the database URL to the VM.' }
            $Passthrough.AddRange([string[]]@('--database-url-file', "$inputs/database-url"))
        }
        if ($DatabaseCaFile) {
            if (-not (Test-Path -LiteralPath $DatabaseCaFile -PathType Leaf)) { Die "Cannot read --database-ca-file $DatabaseCaFile." }
            $caScript = "umask 077 && mkdir -p $inputs && cat > $inputs/database-ca.pem.new && mv -f $inputs/database-ca.pem.new $inputs/database-ca.pem"
            if ((Send-VmInput (Resolve-Path -LiteralPath $DatabaseCaFile).Path @('sudo', '-n', 'sh', '-c', (Quote-Posix $caScript))) -ne 0) { Die 'Could not copy the database CA certificate to the VM.' }
            $Passthrough.AddRange([string[]]@('--database-ca-file', "$inputs/database-ca.pem"))
        }
        # The IRC operator's passwords are for this run only: checked here, copied to the VM only after the
        # installer, never into the fleet's inputs, and removed when the run ends, however it ends.
        $operatorDir = if ($dryRun) { $inputs } else { '/root/subspace-setup-operator' }
        $operatorFiles = @(@('irc-operator-password', $IrcOperatorPasswordFile), @('irc-operator-account-password', $IrcOperatorAccountPasswordFile), @('api-enrolment-token', $ApiEnrolmentTokenFile))
        foreach ($pair in $operatorFiles) {
            $option, $file = $pair
            if ($file -and -not (Test-Path -LiteralPath $file -PathType Leaf)) { Die "Cannot read --$option-file $file." }
        }

        # The dry run's installer copy goes beside its files and is removed with them, so the installed
        # /opt/subspace/bootstrap and the subspace-setup command stay as the last real run left them.
        if ($dryRun) { Send-Payload "$inputs/bootstrap" } else { Send-Payload }
        foreach ($pair in $operatorFiles) {
            $option, $file = $pair
            if (-not $file) { continue }
            $transient.Add($operatorDir)
            $operatorScript = "umask 077 && mkdir -p $operatorDir && cat > $operatorDir/$option"
            if ((Send-VmInput (Resolve-Path -LiteralPath $file).Path @('sudo', '-n', 'sh', '-c', (Quote-Posix $operatorScript))) -ne 0) { Die "Could not copy the --$option-file to the VM." }
            $Passthrough.AddRange([string[]]@("--$option-file", "$operatorDir/$option"))
        }

        $setupArgs = @('setup')
        if (-not (Test-Interactive)) { $setupArgs += '--non-interactive' }
        $setupArgs += $Passthrough
        $quoted = ($setupArgs | ForEach-Object { Quote-Posix $_ }) -join ' '
        if ($dryRun) {
            Say "Running subspace-setup --dry-run on the VM from $inputs, which is removed afterwards."
            $status = Invoke-Vm @('sudo', "$inputs/bootstrap/subspace-installer/vm/setup.sh", $quoted) -Tty
            if ($status -ne 0) { Die "The dry run failed; read subspace-setup's output above." }
            Say 'Dry run complete. Run the same command without --dry-run to install.'
            return
        }
        Say 'Running subspace-setup on the VM. This takes a while the first time.'
        $setupStatus = Invoke-Vm @('sudo', $SetupCmd, $quoted) -Tty
        if ($setupStatus -ne 0) { Die 'subspace-setup failed on the VM; read its output above.' }
    } finally {
        if ($transient.Count -and (Invoke-Vm (@('sudo', '-n', 'rm', '-rf') + @($transient | Select-Object -Unique))) -ne 0) {
            Write-Host "    Remove $($transient -join ' ') on the VM by hand: they hold this run's copies, such as the IRC operator's password."
        }
    }

    Save-Private (Join-Path $State 'summary.env') @('sudo', '-n', 'cat', '/root/fleet/summary.env')
    Confirm-Lockdown
    $s = Read-Env (Join-Path $State 'summary.env')
    if ($s.SUBSPACE_IRC_MODE -eq 'local') {
        Save-Private (Join-Path $State 'irc.password') @('sudo', '-n', 'subspace-setup', 'secret', 'irc-admin')
    } elseif ($IrcTarget) {
        # The bundle holds every agent's IRC password: an owner-only file, streamed over SSH, then deleted.
        $bundle = Join-Path $State 'irc-bundle.json'
        try {
            Save-Private $bundle @('sudo', '-n', 'subspace-setup', 'irc-bundle')
            Say "Adding the fleet's accounts and channels to the shared IRC server '$IrcTarget'."
            Invoke-OnTarget $IrcTarget {
                if ((Send-VmInput $bundle @('sudo', '-n', 'subspace-setup', 'irc-server', 'add', '-')) -ne 0) {
                    Die 'Could not add the fleet to the shared IRC server.'
                }
                Save-Private (Join-Path $State "$($s.SUBSPACE_ADMIN_ACCOUNT).password") @('sudo', '-n', 'subspace-setup', 'irc-server', 'secret', $s.SUBSPACE_ADMIN_ACCOUNT)
            }
            Write-Private (Join-Path $State 'irc-target') "$IrcTarget`n"
        } finally { Remove-Item -Force $bundle -ErrorAction SilentlyContinue }
    }
    if ($s.SUBSPACE_STARGATE_API -eq '1') {
        Save-Private (Join-Path $State 'stargate-api.token') @('sudo', '-n', 'subspace-setup', 'stargate-token')
    }
    if ($ApiTarget) {
        # Register this fleet with the shared API; the registration holds no secrets.
        $registration = Join-Path $State 'fleet-registration.json'
        try {
            Save-Private $registration @('sudo', '-n', 'subspace-setup', 'fleet-registration')
            Say "Registering the fleet with the Subspace API on '$ApiTarget'."
            Invoke-OnTarget $ApiTarget {
                if ((Send-VmInput $registration @('sudo', '-n', 'subspace-setup', 'register-fleet', '-')) -ne 0) {
                    Die "Could not register the fleet with the Subspace API on '$ApiTarget'."
                }
            }
        } finally { Remove-Item -Force $registration -ErrorAction SilentlyContinue }
        Write-Private $apiFile "$ApiTarget`n"
    }
    if ($hosted) { Remove-Item -Force -ErrorAction SilentlyContinue $apiFile }
    Write-DesktopConfig $IrcTarget
    Say "Done. Fleet '$($s.SUBSPACE_FLEET_NAME)' is running on a private VM."
    Note "1. Sign in to the fleet's shared CLIProxyAPI for the models you chose:"
    Note "   Claude: .\install.ps1 providers login claude --target $Target"
    Note "   Codex:  .\install.ps1 providers login codex-device --target $Target"
    Note "   Check existing accounts with .\install.ps1 providers --target $Target."
    Note "2. Start the tunnel:   .\install.ps1 tunnel --target $Target --background"
    Note '3. Run the desktop app (it signs in to 127.0.0.1:3000 as the admin).'
    Note "4. Check privacy:      .\install.ps1 audit --target $Target"
}

function Invoke-IrcServer {
    Connect-Target
    Send-Payload
    $setupArgs = @('irc-server', 'setup')
    if (-not (Test-Interactive)) { $setupArgs += '--non-interactive' }
    $setupArgs += $Passthrough
    $quoted = ($setupArgs | ForEach-Object { Quote-Posix $_ }) -join ' '
    if ((Invoke-Vm @('sudo', $SetupCmd, $quoted) -Tty) -ne 0) { Die 'IRC server setup failed on the host; read its output above.' }
    Save-Private (Join-Path $State 'summary.env') @('sudo', '-n', 'cat', '/root/irc/summary.env')
    Confirm-Lockdown
    $ca = Get-VmOutput @('sudo', '-n', 'subspace-setup', 'irc-server', 'ca')
    [IO.File]::WriteAllText((Join-Path $State 'ca.pem'), "$ca`n")
    Say "Shared IRC server '$Target' is ready. Add fleets with: .\install.ps1 --irc-server $Target ..."
}

function Invoke-Skills {
    # Local agents (Claude Code, Codex, OpenCode) on this PC: skills, helpers and MCP.
    $agents = @(); $person = ''
    for ($n = 0; $n -lt $Passthrough.Count; $n++) {
        switch ($Passthrough[$n]) {
            '--agents' { $n++; $agents = @($Passthrough[$n] -split ',' | Where-Object { $_ }) }
            '--name' { $n++; $person = $Passthrough[$n] }
            default { Die "Unknown skills option $($Passthrough[$n])." }
        }
    }
    if (-not $agents.Count) { $agents = @('claude', 'codex', 'opencode' | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue }) }
    if (-not $agents.Count) { Die 'No Claude Code, Codex or OpenCode found. Pass --agents to choose.' }
    # Windows ships a python.exe alias that only opens the Store, so check that it really runs.
    $python = @('py', 'python', 'python3') | Where-Object {
        (Get-Command $_ -ErrorAction SilentlyContinue) -and ((Invoke-Quiet { & $_ -c 'import sys; print(sys.version_info[0])' }) -contains '3')
    } | Select-Object -First 1
    if (-not $python) { Die "The skills' helpers need Python 3 (winget install Python.Python.3.12)." }
    $hosted = & $python (Join-Path $Root 'agent-skills\bin\subspace_token.py') --workos-mcp-url
    if ($LASTEXITCODE -ne 0) { Die 'Check the local agent configuration.' }
    if (-not $hosted) {
        Get-TargetEnv | Out-Null
        if (-not (Test-Path (Join-Path $State 'summary.env'))) { Die 'Install the fleet first.' }
    }
    $bin = Join-Path $SubspaceHome 'bin'
    $agentsEnv = Join-Path $SubspaceHome 'agents.env'
    if (-not $person -and (Test-Path $agentsEnv)) { $person = (Read-Env $agentsEnv).SUBSPACE_HUMAN_NAME }
    if (-not $person) { $person = (Read-Env (Join-Path $State 'summary.env')).SUBSPACE_ADMIN_ACCOUNT }
    if ($hosted -and -not $person) { Die 'Pass --name for the person these agents act for.' }
    New-Item -ItemType Directory -Force -Path $bin | Out-Null
    Copy-Item -Force (Join-Path $Root 'agent-skills\bin\subspace_token.py'), (Join-Path $Root 'agent-skills\bin\subspace_irc.py') $bin
    [IO.File]::WriteAllText($agentsEnv, "SUBSPACE_TARGET=$Target`nSUBSPACE_HUMAN_NAME=$person`n")
    Say "Local agents will sign in as you and mark their messages 'on behalf of $person (<model>)'."
    $mcp = if ($hosted) { $hosted } else { 'http://127.0.0.1:3000/mcp' }
    $token = (Join-Path $bin 'subspace_token.py') -replace '\\', '/'
    # Claude Code runs headersHelper through a shell, so quote both paths: they may contain spaces.
    $pythonExe = (@(Invoke-Quiet { & $python -c 'import sys; print(sys.executable)' }) -join '').Trim()
    if (-not $pythonExe) { $pythonExe = $python }
    $helper = '"' + ($pythonExe -replace '\\', '/') + '" "' + $token + '" --header'
    # A hosted registration pins its URL; the development registration stays as it was.
    if ($hosted) { $helper += ' --mcp-endpoint "' + $mcp + '"' }
    foreach ($agent in $agents) {
        $dest = switch ($agent) {
            'claude' { Join-Path $HOME '.claude\skills' }
            'codex' { if ($env:CODEX_HOME) { Join-Path $env:CODEX_HOME 'skills' } else { Join-Path $HOME '.codex\skills' } }
            'opencode' { Join-Path $HOME '.config\opencode\skills' }
            default { Die "Unknown agent $agent (use claude, codex or opencode)." }
        }
        New-Item -ItemType Directory -Force -Path $dest | Out-Null
        foreach ($skill in 'subspace-irc', 'subspace-tracker') { Remove-Item -Recurse -Force (Join-Path $dest $skill) -ErrorAction SilentlyContinue }
        Copy-Item -Recurse (Join-Path $Root 'agent-skills\subspace-irc') $dest
        Copy-Item -Recurse (Join-Path $Root "agent-skills\$agent\subspace-tracker") $dest
        switch ($agent) {
            'claude' {
                if (Get-Command claude -ErrorAction SilentlyContinue) {
                    $json = ([ordered]@{ type = 'http'; url = $mcp; headersHelper = $helper } | ConvertTo-Json -Compress)
                    # Windows PowerShell 5.1 (and .cmd shims) drop unescaped double quotes from native
                    # arguments, so find the form that works under a scratch name before replacing the
                    # existing entry: as is, or escaped as the Microsoft C runtime expects.
                    $working = $null
                    $escaped = $json -replace '(\\*)"', '$1$1\"'
                    foreach ($form in @($json, $escaped, ('"' + $escaped + '"'))) {
                        Invoke-Quiet { & claude mcp remove --scope user subspace-setup-check } | Out-Null
                        Invoke-Quiet { & claude mcp add-json --scope user subspace-setup-check $form } | Out-Null
                        if ($LASTEXITCODE -eq 0) { $working = $form; break }
                    }
                    Invoke-Quiet { & claude mcp remove --scope user subspace-setup-check } | Out-Null
                    if ($working) {
                        # Removing fails harmlessly when there is no entry yet.
                        Invoke-Quiet { & claude mcp remove --scope user subspace } | Out-Null
                        Invoke-Quiet { & claude mcp add-json --scope user subspace $working } | Out-Null
                    }
                    if (-not $working -or $LASTEXITCODE -ne 0) {
                        Write-Warning 'Could not add the Subspace MCP server to Claude Code; any earlier entry is unchanged. Add it with:'
                        Note "claude mcp add-json --scope user subspace '$json'"
                        $script:SkillsFailed = $true
                    }
                }
            }
            'codex' {
                if (Get-Command codex -ErrorAction SilentlyContinue) {
                    # Removing fails harmlessly when there is no entry yet.
                    Invoke-Quiet { & codex mcp remove subspace } | Out-Null
                    if ($hosted) {
                        Invoke-Quiet { & codex mcp add subspace -- $pythonExe $token --mcp-stdio --mcp-endpoint $mcp } | Out-Null
                    } else {
                        Invoke-Quiet { & codex mcp add subspace --url $mcp --bearer-token-env-var SUBSPACE_API_TOKEN } | Out-Null
                    }
                    if ($LASTEXITCODE -ne 0) {
                        Write-Warning 'Could not add the Subspace MCP server to Codex. See agent-skills\README.md.'
                        $script:SkillsFailed = $true
                    }
                }
            }
            'opencode' {
                $file = Join-Path $HOME '.config\opencode\opencode.json'
                try {
                    $config = if (Test-Path $file) { Get-Content $file -Raw | ConvertFrom-Json } else { [pscustomobject]@{ '$schema' = 'https://opencode.ai/config.json' } }
                    if (-not ($config.PSObject.Properties.Name -contains 'mcp')) { $config | Add-Member mcp ([pscustomobject]@{}) }
                    $server = if ($hosted) {
                        @{ type = 'local'; command = @($pythonExe, $token, '--mcp-stdio', '--mcp-endpoint', $mcp); enabled = $true }
                    } else {
                        @{ type = 'remote'; url = $mcp; enabled = $true
                           headers = [pscustomobject]@{ Authorization = 'Bearer {env:SUBSPACE_API_TOKEN}' } }
                    }
                    $config.mcp | Add-Member -Force subspace ([pscustomobject]$server)
                    [IO.File]::WriteAllText($file, ($config | ConvertTo-Json -Depth 12))
                } catch { Note "Could not edit opencode.json (comments?). Add mcp.subspace by hand; see agent-skills\README.md." }
            }
        }
        Say "Installed the Subspace skills for $agent in $dest."
    }
    if ($hosted) {
        Note "Sign in once: & $python `"$token`" --login"
    } else {
        Note 'Claude Code refreshes its API token by itself. For Codex and OpenCode, start them with:'
        Note "  `$env:SUBSPACE_API_TOKEN = (& $python $token)"
        Note "Keep the tunnel running: .\install.ps1 tunnel --target $Target --background"
    }
    if ($SkillsFailed) { Die 'The skills are installed, but an MCP registration failed (see the warnings above).' }
}

function Invoke-Providers {
    Get-TargetEnv | Out-Null
    Update-SshConfig
    if ($Passthrough.Count -lt 2 -or $Passthrough[0] -ne 'login') {
        Invoke-Vm @('sudo', '-n', 'subspace-setup', 'providers', 'status') | Out-Null
        return
    }
    $provider = $Passthrough[1]
    if ($provider -notmatch '^[a-z-]+$') { Die 'Unknown provider.' }
    $port = switch ($provider) { 'claude' { '54545' } 'codex' { '1455' } default { '' } }
    if ($port) { Say "Forwarding port $port so the $provider login can finish in your browser." }
    exit (Invoke-Vm @('sudo', 'subspace-setup', 'providers', 'login', $provider) -Tty -Forward $port)
}

switch ($Command) {
    'install' { Invoke-Install }
    'providers' { Invoke-Providers }
    'skills' { Invoke-Skills }
    'irc-server' { Invoke-IrcServer }
    'tunnel' { Start-Tunnel }
    'desktop' {
        Get-TargetEnv | Out-Null
        Update-SshConfig
        if ((Read-Env (Join-Path $State 'summary.env')).SUBSPACE_STARGATE_API -eq '1' -and (Invoke-Vm @('true')) -eq 0) {
            Save-Private (Join-Path $State 'stargate-api.token') @('sudo', '-n', 'subspace-setup', 'stargate-token')
        }
        Write-DesktopConfig $IrcTarget
    }
    'ssh' {
        Get-TargetEnv | Out-Null; Update-SshConfig
        # SSH options go before the host and a command after it: .\install.ps1 ssh uptime. PowerShell drops a
        # '--' typed at its prompt, so the command starts at the first word that is neither an option nor the
        # value of an ssh option that takes one; a '--' that does arrive (pwsh -File) also ends the options.
        $options = New-Object System.Collections.Generic.List[string]
        $command = New-Object System.Collections.Generic.List[string]
        for ($n = 0; $n -lt $Passthrough.Count; $n++) {
            $word = [string]$Passthrough[$n]
            if ($word -eq '--') {
                if ($n + 1 -lt $Passthrough.Count) { $command.AddRange([string[]]@($Passthrough)[($n + 1)..($Passthrough.Count - 1)]) }
                break
            }
            if (-not $word.StartsWith('-')) { $command.AddRange([string[]]@($Passthrough)[$n..($Passthrough.Count - 1)]); break }
            $options.Add($word)
            if ($word -cmatch '^-[BbcDEeFIiJLlmOoPpQRSWw]$' -and $n + 1 -lt $Passthrough.Count) { $n++; $options.Add([string]$Passthrough[$n]) }
        }
        & ssh.exe -F (Join-Path $State 'ssh_config') @options subspace-target @command
        exit $LASTEXITCODE
    }
    'audit' { Get-TargetEnv | Out-Null; Update-SshConfig; exit (Invoke-Vm @('sudo', '-n', 'subspace-setup', 'audit')) }
    'status' { Get-Content (Join-Path $State 'target.env'); Update-SshConfig; Invoke-Vm @('sudo', '-n', 'subspace-setup', 'status') | Out-Null }
    { $_ -in @('stop', 'start') } {
        $t = Get-TargetEnv
        if ($t.KIND -ne 'multipass') { Die 'Only local VMs can be stopped or started here.' }
        & multipass $Command $t.VM
    }
    'destroy' {
        $t = Get-TargetEnv
        if ($t.KIND -ne 'multipass') { Die 'Delete remote VMs with your cloud provider. This only removes local VMs.' }
        if (-not (Test-Interactive)) { Die 'destroy asks for confirmation; run it in a terminal.' }
        if ((Read-Host "Type the VM name ($($t.VM)) to delete it and its saved credentials") -ne $t.VM) { Die 'Not deleted.' }
        & multipass delete --purge $t.VM
        Remove-Item -Recurse -Force $State
        Say "Deleted $($t.VM)."
    }
}
