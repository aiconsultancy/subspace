# Install Subspace

This page gets a Subspace fleet running and connects the desktop app to it.
Clone the repository, open your coding agent (Claude Code, Codex or another)
in it, and ask it to set Subspace up. The agent reads this page, asks you a few
questions and runs the installer for you. You can also follow it yourself.

We recommend a fleet VM in your cloud, reached over a private network
(Tailscale, Cloudflare Zero Trust or similar): Subspace is meant to be
multiplayer, and that keeps the agents' builds off your machine. Your agent
can create the VM with your cloud provider's own tools and set the private
network up for you, with your permission. To just try Subspace out, a VM on
your own computer is the quickest way, and it stays fully supported.

> [!IMPORTANT]
> **Coding agents: read this whole page before running anything.**
>
> 1. **Ask first, all at once.** Ask the user every question in
>    [Decide with the user](#decide-with-the-user) before you run, install or
>    create anything, and explain the trade-offs in your own words. Do not pick
>    names for them.
> 2. **Confirm the plan once.** Summarise the answers, the exact commands you
>    will run and everything they will install or change: software on this
>    computer (Lima, QEMU, Multipass, Tailscale, `cloudflared`), the VM and
>    any other cloud resources with their cost, the cloud account and private
>    network they use, the user's agent profiles. Wait for one clear yes.
>    That yes covers only what the plan lists; pass `--install-deps` only if
>    it says so.
> 3. **Then work unattended to the end.** Run the installer non-interactively:
>    pass the answers as flags and add `--yes`. Its prompts cannot reach the
>    user through you. Do not stop to ask again. If something the plan did
>    not cover comes up, take the safe default named in
>    [Decide with the user](#decide-with-the-user), or stop with a clear
>    message that says what is needed and which command to run next.
> 4. **The fleet VM must stay private.** Never open its services to the
>    internet, never bind them to `0.0.0.0`, and never pass `--no-lockdown`
>    unless the user asks for it after you have explained the risk. Agents'
>    keys and model logins live on that VM. A shared IRC server stays just as
>    private: reachable over the private network only.
> 5. Never print, paste or log passwords, tokens or private keys. The
>    installer saves them as owner-only files and tells you where.
> 6. Finish by running the [privacy audit](#check-that-the-fleet-is-private)
>    and reporting its result. Then tell the user that **the agents stay
>    offline until CLIProxyAPI is signed in to at least one of their provider
>    accounts**. That sign-in needs the user's browser and comes after the
>    install, so you cannot do it unattended: walk them through
>    [bringing the agents online](#bring-the-agents-online) if they are there,
>    or leave them the exact commands.
> 7. If the plan included the [local agent skills](#use-your-local-agents-with-the-fleet),
>    install them last, for the agents and the name the user gave.

## What you are installing

```mermaid
flowchart LR
  subgraph You["Your computer"]
    D[Subspace desktop app]
  end
  subgraph VM["Fleet VM (private): in your cloud, or on your computer to try it"]
    SG[Stargate]
    RA[Rear admiral]
    C[Captains + crew]
    P[CLIProxyAPI]
    API[Subspace API + PostgreSQL]
    IRC["Ergo IRC (or one shared server for all fleets)"]
  end
  D -- "SSH tunnel, over Tailscale or Cloudflare Zero Trust for a cloud VM" --> VM
  SG --- RA & C
  SG --- IRC
  RA & C --- P
  D -. "MCP, through the tunnel" .- API
```

- **A fleet is one Linux VM**, in your cloud on a private network
  (recommended) or on your computer (to try it). It runs a **rear admiral**
  and one or more **captains**. Each is its own Linux user with its own tmux
  session, files, IRC account and GitHub App. Each captain has a **crew**
  (two Claude-family and two Codex-family workers by default) that runs
  under the captain's Linux user, inside the captain's OpenCode server. Each crew member has one
  Git worktree per repository, commits as its captain and is reached only
  through its captain.
- **Stargate is the glue.** It connects the rear admiral and captains to
  their IRC channels (and the rear admiral to Slack or Teams, if you add them
  later), routes messages and wakes the right agent. Its crew plugin lets each
  captain delegate to its crew and collect their results.
- **CLIProxyAPI** holds your agentic provider accounts (for example a Claude
  or ChatGPT subscription) and serves the models to the agents. You sign it in
  after the install; until then the agents are offline.
- **OpenCode** is the agent harness: the program each agent runs in. The
  installer installs it and uses it by default. Stargate can also drive agents
  through the **Claude Agent SDK** or the **OpenAI Codex SDK**, but OpenCode is
  the most tested and the recommended choice. The Stargate dashboard and
  **AgentsView** show what the agents are doing.
- **Ergo IRC** carries conversations between you and the agents. Every fleet
  has the same [standard channels](subspace-installer/README.md#channels):
  **Starbase** for talking to the rear admiral, **fleet command** where the
  rear admiral and the captains coordinate, and a channel per captain
  (ship). It runs on the fleet VM for a trial or a single fleet; for
  real use with several fleets, one shared server in a container serves them
  all ([One fleet or several](#one-fleet-or-several-shared-irc)).
- **The Subspace API** (with PostgreSQL) holds issues, initiatives and fleets.
  The desktop app uses it, and so can your local coding agents through its MCP
  server ([Use your local agents](#use-your-local-agents-with-the-fleet)).
  The fleet's own agents reach it only through its agent listener, a second
  port where each acts as its own agent record and never as an administrator;
  the API's main port and its development sign-in are for you only
  ([Who can reach the API](#who-can-reach-the-api)).
- **The desktop app** runs on your computer and reaches all of this through a
  private connection.

Everything on the VM listens on `127.0.0.1` only. The installer adds a
firewall that drops all inbound traffic except SSH, hardens SSH to key-only
logins for your user, and keeps the agents' Linux users away from operator
services, including the API's main port, and from the cloud's metadata
service, so they cannot use the VM's cloud identity.

## Decide with the user

Ask all of these before running anything, then confirm the plan once.
Defaults are in bold. Each question says what to do if the user cannot
answer it now.

1. **Where should the fleet run?** Explain the choice. For real use we
   recommend a VM in the user's cloud, reached over a private network:
   Subspace is meant to be multiplayer, and a cloud VM keeps the agents'
   builds off their machine. A VM on this computer is the quickest way to try
   Subspace out, and stays fully supported, though the agents then share the
   machine. **If the user cannot answer, use a new local VM**: it needs no
   cloud account, no spending and no private network, and you never create
   cloud resources unless the user chose to.
   - **A new VM in their cloud (recommended).** You create it with the
     provider's own CLI under the user's signed-in account, as
     [Create a cloud VM](#create-a-cloud-vm) describes; the installer does
     not create cloud VMs. Ask which provider, which account or subscription,
     which region and which size (as for a local VM, below), and confirm that
     its cost is theirs to accept. Ask whether you may use that CLI; if it is
     missing or not signed in, they install it and sign in themselves, or let
     you install it as part of the plan. Settle the private network in
     question 10.
   - **A new local VM on this computer (the quickest way to try it).** On
     macOS it uses Apple's Virtualization framework through
     [Lima](https://lima-vm.io); on Linux,
     Lima with QEMU and KVM; on Windows, [Multipass](https://canonical.com/multipass)
     with Hyper-V (Windows Pro, Enterprise or Education; Windows Home has no
     Hyper-V, so use an existing VM instead). All are free. The VM needs
     about 4 CPUs, 8 GiB of memory and 40 to 60 GiB of disk; use 16 GiB of
     memory for two or more captains. Ask whether you may install the VM
     tool if it is missing (`--install-deps`); without that, the installer
     stops and says what to install.
   - **An existing VM**, in their cloud or elsewhere. Ubuntu 24.04 or
     Debian 12, amd64 or arm64, reachable over SSH with a key and
     passwordless sudo. It must not accept inbound traffic from the internet
     (see [Create a cloud VM](#create-a-cloud-vm)). Collect the SSH target
     (`user@host`), the key to use, the SSH port, any proxy command and the
     host key fingerprint from a trusted channel. Ask whether you may use
     their cloud account to read that fingerprint (for example
     `az vm run-command`); if not, they give it to you. Never run without it.
2. **Where should the IRC server run?** Ask whether they intend to run more
   than one fleet, and explain [the difference](#one-fleet-or-several-shared-irc).
   For real use we recommend **one IRC server (Ergo) for the whole setup,
   however many fleets there are, running in a container** on the private
   network. Every fleet and every desktop shares it, and it is never
   reachable from the internet.
   - **A local VM to try Subspace out (question 1):** Ergo runs on the fleet
     VM, beside the API and one PostgreSQL that holds the API's database and
     IRC's history. That is the installer's default; there is nothing to
     choose.
   - **A cloud fleet that will only ever be one fleet:** Ergo on the same VM
     is fine, exactly as for a local VM.
   - **A cloud install of more than one fleet, or one that may grow:** run one
     shared IRC server first, then point every fleet at it. On Azure we
     recommend [Azure Container Apps](#example-azure-container-apps), which
     you set up with the Azure CLI by following
     [Run the shared IRC server in a container](#run-the-shared-irc-server-in-a-container);
     the installer does not deploy it. Ask for the subscription, region and
     virtual network (the fleet VMs' network, or permission to create one),
     and confirm that the cost of the environment, its PostgreSQL server for
     history and its storage is theirs to accept. On any cloud, the installer
     can instead set up a small dedicated VM that runs Ergo in Docker
     (`./install.sh irc-server`); collect that VM's details as in question 1.
     Switching fleet in the desktop then loads that fleet's IRC channels, so
     they work across fleets while seeing only the channels and captains of
     the fleet they are in.
   - **An IRC server they already run:** collect its DNS name, TLS port and
     CA certificate, and who can add the fleets' accounts to it. Officers sign
     in to it by certificate (`--irc remote --irc-host NAME`): setup makes one
     per officer and prints its account, fingerprint and the two NickServ
     lines (`/NS SAREGISTER`, `/NS CERT ADD`) that the server's operator runs.
     If the user holds an IRC operator credential with NickServ's `accreg`
     capability, pass `--irc-operator NAME --irc-operator-password-file FILE`
     (a `chmod 600` file on this computer, removed from the VM after the run)
     and setup registers the accounts and checks each sign-in itself; it
     never prints or keeps the password. If the server lets only signed-in
     accounts connect (Ergo's `require-sasl`), also pass
     `--irc-operator-account NAME --irc-operator-account-password-file FILE`
     for the account the operator signs in to first. Use `--irc-auth password` only for a
     server that cannot take certificates, and then its operator sets each
     officer's password. See
     [IRC sign-in](subspace-installer/README.md#irc-sign-in).

   **If the user cannot answer, put Ergo on the fleet VM**: it creates no
   extra cloud resources and costs nothing more. Tell them that moving to a
   shared server later keeps the fleet but not its IRC history.
3. **Names.** A name for the fleet (for example "Example Fleet"), a name for
   the rear admiral (for example "Atlas") and one or more captain or ship
   names (for example "Orion, Vega"). Names become IRC channels, IRC accounts
   and Linux users, so keep them short: the fleet ID (the fleet name in lower
   case, or `--slug`) plus a hyphen and an agent's name must fit 32
   characters, as in `example-fleet-orion`. A captain cannot be called
   Starbase or Command, the names of two of the fleet's standard channels.
4. **Which GitHub repositories should the captains have checked out?** List
   them in order, as HTTPS URLs; the first is where each captain starts. Every
   captain gets every repository in the list (`--repositories`, at most
   nine). The answer drives:
   - each captain's own checkout of every repository, and the rear admiral's
     read-only reference copies;
   - **one Git worktree per repository for each crew member**, under its
     captain's Linux user. Crew need at least one repository: with none, the
     captains have no crew;
   - the GitHub access the captains' GitHub Apps need. Captains push, and
     their crew push as them, so each captain's App needs Contents write on
     these repositories, plus Pull requests and Issues write to open pull
     requests and update issues. The rear admiral's App needs Contents read.

   Public repositories work straight away; without Apps, the agents can read
   and commit but not push. **Private** repositories need a GitHub App for
   the rear admiral and every captain before the installer can clone them
   (see [GitHub Apps per fleet member](subspace-installer/README.md#github-apps-per-fleet-member)).
   Ask whether those Apps exist and where their keys are. If not, install
   with the public repositories only, and tell the user how to add the private
   ones once the Apps exist. Setup cannot tell a private repository from its
   URL: the dry run warns about every repository Git cannot read without
   credentials, and the install stops before cloning until it is marked
   `"private": true` with the Apps in place, or removed.
   **Default: no repositories, so no crew yet.**
5. **Crew per captain** (**2 Claude, 2 Codex**; 0 to 10 each,
   `--crew-claude`, `--crew-codex`). Explain that crew run inside their
   captain's OpenCode server as the captain's Linux user: they share the
   captain's files, credentials and GitHub App, commit as the captain, and
   have no IRC accounts. The user talks to the captain, who delegates to its
   crew. A captain's crew roster is fixed once the fleet first starts, so
   settle the counts now. Ask which model each kind of crew member uses
   (**`claude-sonnet-5-5` and `gpt-5.4`**), for example
   `--crew-claude-model claude-opus-5-5 --crew-codex-model gpt-6.2-sol`; the
   models, unlike the counts, can change later.
6. **Their IRC account name** (**admiral**), used as their nickname. The
   agents only take orders from this account.
7. **Demo data?** (**yes**) Fictional fleets, initiatives and issues so they can
   look around the desktop app straight away.
8. **Agent harness** (**OpenCode, recommended**). Stargate also supports the
   Claude Agent SDK and the OpenAI Codex SDK. OpenCode is the most tested,
   and crew need an OpenCode captain. To use the Claude Agent SDK for the
   rear admiral or a captain without crew, set its `harness` to `claude-sdk`
   in `/root/fleet/fleet.json` after the install (see the
   [fleet installer's guide](subspace-installer/README.md#crew-and-repositories)).
   The Codex SDK is configured in Stargate directly; the installer does not
   set it up yet.
9. **Which provider accounts** will power the agents, for example a ChatGPT
   (Codex) or Claude subscription. They sign them in to CLIProxyAPI after the
   install, in their browser; see [Bring the agents online](#bring-the-agents-online).
   Ask whether they will be there at the end to do it. If not, finish the
   install and leave them the sign-in commands.
10. **How they reach the VM.** For a cloud VM a private network is required:
    we recommend **Tailscale** or Cloudflare Zero Trust (free for up to 50
    users), which you set up for them as in
    [Set up the private network](#set-up-the-private-network), with
    `--access tailscale` or `--access cloudflare`. If they have no
    preference, use Tailscale: it needs no domain of their own. For a local
    VM the default is the **SSH tunnel**, which needs nothing else. Either
    way the desktop's ports travel through the SSH tunnel, which for a cloud
    VM runs over the private network. Ask for any addresses that alone may
    use SSH (`--ssh-from`). See [Private access](docs/private-access.md).
    Also offer nginx with internal host names if they want browser-friendly
    addresses.

    The private network needs the user in their browser: signing in to
    Tailscale or Cloudflare, approving this computer, and creating a one-off
    auth key or a tunnel token. Ask whether they are there to do it now.
    Do it together before the unattended run starts, because that run
    cannot reach a VM with no public IP without it: before you confirm the
    plan, or straight after its yes for any step that needs Tailscale or
    `cloudflared` installed on this computer first. If they cannot, do not
    create the cloud VM: offer the local VM instead, or stop and leave them
    the steps.
11. **Secrets.** (**On the VM**, as root-only and per-agent files.) On Azure,
    they can also keep the generated secrets in Key Vault (`--key-vault`), so
    a rebuilt VM gets the same ones. That needs the VM's managed identity to
    have access to the vault, which they set up or allow you to set up in
    their cloud account; if neither, keep the secrets on the VM.
12. **Local agent skills.** Should their own coding agents (Claude Code,
    Codex, OpenCode) get Subspace skills in their user profile, so they can
    use the tracker and talk to the fleet? Which agents, and what name should
    those agents use for them? You install them at the end. See
    [Use your local agents with the fleet](#use-your-local-agents-with-the-fleet).
    **Default: no.**
13. **Slack or Teams** (optional). Should stakeholders reach the rear admiral
    in Slack or Microsoft Teams? Slack works with a private VM as is
    ([Slack](docs/slack.md)). Teams needs an Azure VM and one public callback
    path ([Teams](docs/teams.md)). Teams bots cannot talk to each other, so
    captains coordinate in IRC, and the installer connects only the rear
    admiral to Teams. Both need an app the user creates and its tokens, so
    they follow those guides with the user after the fleet runs. **Default:
    neither.**
14. **Agent dispatch?** (**no**, `--agent-dispatch` to switch it on) With it
    on, issues flagged *Ready for agents* are offered to the captains in
    fleet command, and issues assigned to a captain are announced to it
    there (see [Agent dispatch](README.md#agent-dispatch)). It needs the
    Subspace API on this VM and repositories, because only a captain with
    crew can accept an offer. Setup registers the rear admiral and captains
    as the fleet's API agents in any case; with dispatch on it also gives the
    rear admiral's gateway a development sign-in token, which lasts eight hours: re-run the installer
    to renew it, or see
    [Agent dispatch](subspace-installer/README.md#agent-dispatch) for a
    long-lived token. An unattended run leaves it off.
15. **An existing PostgreSQL for the tracker?** (**no**: the API keeps its
    own PostgreSQL on the fleet VM.) If they already run one, for example a
    managed server, the fleet's API can use it instead, with no database on
    the VM. Ask for a file on this computer holding its
    `postgresql://USER:PASSWORD@HOST:5432/DATABASE?sslmode=verify-full` URL
    (`chmod 600`; never paste the URL into the chat), its CA certificate when
    the VM does not trust it, whether its migrations may be applied (back it
    up first), and the `--organisation` and `--admin-email` of someone who is
    already an administrator there, unless the database is empty. Pass
    `--database-url-file FILE`, `--database-ca-file FILE` and, to migrate it,
    `--migrate`. The VM must reach the server, so its firewall must allow the
    VM. Do a `--dry-run` first: it connects read-only and says whether
    migrations are missing and whether that person is an administrator. There
    is no demo data in a database the installer did not create, and no way
    back to a database on the VM. See
    [An existing PostgreSQL](subspace-installer/README.md#an-existing-postgresql).

Then confirm the plan once, as the box at the top of this page describes,
and run without stopping to ask again.

## Install on a cloud VM

This is the recommended setup. You create the VM with the cloud provider's
own CLI by following this section, put it on a private network, then run the
installer against it over SSH. The installer itself does not create cloud VMs
or install Tailscale or `cloudflared`; it installs the fleet onto a VM you can
already reach, and `--access` limits SSH to the private network you chose.

### Create a cloud VM

These rules hold for any provider. For an existing VM, check that it meets
the same image, SSH, inbound and host key rules; if it must keep a public IP
for now, allow SSH from the user's own address only in the cloud firewall,
then move to Tailscale or Cloudflare and remove that rule.

- **Only what the plan says.** Use the provider, account or subscription,
  region and size the user chose, at the cost they accepted, through the
  provider's official CLI (for example `az`, `gcloud` or `aws`) under the
  user's existing sign-in. Never create credentials (service principals,
  access keys, roles or API tokens) and never change account-level settings
  such as quotas, billing or policies. If the work needs one of those, stop
  and say what.
- **A supported image:** Ubuntu 24.04 LTS or Debian 12, amd64 or arm64.
- **Key-only SSH for a sudo user.** Give the VM the public half of an SSH key
  on this computer, a new one for the fleet or one the user names, for a
  user with passwordless sudo. Never set a password.
- **No public IP and no inbound rules.** Create the VM without a public
  address, in a firewall or security group that allows no inbound traffic.
  It still needs outbound internet access for packages, the private network,
  GitHub and the model providers. If the provider's network gives none by
  default, its outbound-only NAT service fixes that; it costs money, so it
  belongs in the plan.
- **Reach it through the provider first.** With no public IP, use the
  provider's run-command (or its serial console) to install Tailscale or
  `cloudflared` and join the private network, as in
  [Set up the private network](#set-up-the-private-network). From then on
  the installer uses SSH over that network.
- **Host key from a trusted channel.** Read the VM's SSH host key fingerprint
  through the same run-command, never from a first SSH connection, and pass
  it to the installer as `--host-key`.
- **Record what you created.** Keep everything in one resource group,
  project or similar where the provider has one, and tag it (for example
  `app=subspace`). In your final report, list the provider, account, region,
  each resource's name, and the VM's name in the tailnet or its Cloudflare
  tunnel, so the user can find and delete them.
- **Never delete cloud resources without asking**, even to retry a failed
  step. If creation fails part-way, stop and report what exists.

### Set up the private network

[Private access](docs/private-access.md) has the commands and policies for
both documented routes; follow it rather than improvising.

- **Tailscale.** The user, in their browser: signs in to Tailscale, adds
  `tag:fleet` to the tailnet policy as Private access shows, signs this
  computer in to the tailnet, and creates a one-off, pre-approved auth key
  with the tag `tag:fleet`. You, with the plan's permission: install
  Tailscale on this computer if it is missing, install it on the VM through
  the provider's run-command and join the VM with that key. Then pass
  `--remote <user>@<vm>.<tailnet>.ts.net --access tailscale`.
- **Cloudflare Zero Trust.** The user, in their browser: creates a tunnel
  and copies its token, adds an SSH-only public hostname on a domain in
  their Cloudflare account, creates an Access application that allows only
  them, and, once `cloudflared` is on this computer, signs in to it there
  (`cloudflared access login https://fleet-ssh.example.com`). You, with the
  plan's permission: install `cloudflared` on this computer if it is
  missing, install it on the VM through the run-command and run
  `cloudflared service install` with the token. Then pass `--access
  cloudflare` and `--ssh-proxy`, as Private access shows.

Auth keys and tunnel tokens are secrets. Have the user save them in an
owner-only file, pass them to the run-command without printing them, and
delete your copies afterwards. Prefer a one-off key with a short expiry,
because the provider may keep a copy of the run-command's script.

### Example: Azure

Check the subscription with `az account show` (select the user's with
`az account set --subscription <subscription>`), then create the VM without a
public IP and with a network security group that has no inbound allow rules:

```sh
az group create --name subspace-fleet-rg --location <region> --tags app=subspace
az vm create --resource-group subspace-fleet-rg --name subspace-fleet \
  --image Ubuntu2404 --size Standard_D4s_v5 --os-disk-size-gb 64 \
  --admin-username azureuser --ssh-key-values ~/.ssh/subspace-fleet.pub \
  --public-ip-address "" --nsg-rule NONE --tags app=subspace
```

`Standard_D4s_v5` has 4 CPUs and 16 GiB of memory; use the size the user
chose. Join the VM to the tailnet, or install `cloudflared`, through a
run-command (`az vm run-command invoke --resource-group subspace-fleet-rg
--name subspace-fleet --command-id RunShellScript --scripts ...`) with the
commands from [Private access](docs/private-access.md). If that cannot reach
the package repositories, the VM has no outbound access: Azure is retiring
default outbound access for new virtual networks, so the subnet needs a NAT
gateway, which the user must agree to. `subspace-setup` checks this first, in
a dry run too: without outbound HTTPS to GitHub (and `download.docker.com`
when the VM runs containers) or HTTP to the package mirror, it stops at once
and names what to open. Then read the host key:

```sh
az vm run-command invoke -g subspace-fleet-rg -n subspace-fleet --command-id RunShellScript \
  --scripts "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub"
```

`az group delete --name subspace-fleet-rg` deletes everything above. Run it
only when the user asks, and remove the VM from the tailnet or delete its
Cloudflare tunnel too.

### Run the installer

```sh
./install.sh --remote azureuser@fleet.example.ts.net --host-key SHA256:... --yes \
  --access tailscale \
  --name "Example Fleet" --rear-admiral Atlas --captains Orion,Vega --admin admiral --demo
```

Add `--identity ~/.ssh/key` for a specific key (load passphrase-protected keys
into `ssh-agent` first), `--ssh-port` for another port (the lockdown keeps
whichever ports sshd listens on), and
`--ssh-from 203.0.113.7/32` to accept SSH from listed addresses only.
`--access tailscale` accepts SSH on the tailnet only; through Cloudflare, use
`--access cloudflare` with the `--ssh-proxy` from
[Private access](docs/private-access.md#cloudflare-zero-trust) instead. Leave
`--access` out only for an existing VM you reach another private way.

For an existing PostgreSQL (question 15), add `--database-url-file FILE`
(`--database-ca-file FILE` for a private CA, `--migrate` to apply missing
migrations), and the `--organisation` and `--admin-email` of an
administrator already in that database. Run it with `--dry-run` first: the
dry run checks the database read-only and changes nothing on the VM.

To keep the generated secrets in Azure Key Vault, give the VM a managed
identity that can read and write the vault's secrets (for example the Key
Vault Secrets Officer role on that vault), then add `--key-vault <vault-name>`.
Secrets are named `subspace-<fleet>-…`. The VM still writes root-only copies,
because the fleet's services read them from disk. The lockdown keeps the
agents' Linux users away from Azure's metadata service, so they cannot ask for
that identity's tokens; the audit fails if they could.

## Install on a local VM

The quickest way to try Subspace. macOS or Linux, from the repository root:

```sh
./install.sh --local --yes --install-deps \
  --name "Example Fleet" --rear-admiral Atlas --captains Orion,Vega \
  --admin admiral --demo
```

Windows, in PowerShell from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 --local --yes --install-deps `
  --name "Example Fleet" --rear-admiral Atlas --captains Orion,Vega `
  --admin admiral --demo
```

Add the user's repositories as `--repositories URL,URL` (HTTPS Git URLs,
optionally `ID=URL`), their crew counts as `--crew-claude N --crew-codex N`
and the crew models as `--crew-claude-model MODEL --crew-codex-model MODEL`,
on this command and on the cloud VM one above. Leave out `--install-deps`
to stop and ask instead of installing Lima, QEMU or Multipass. Size the VM with `--cpus`, `--memory` and `--disk` (GiB). The first
run downloads an Ubuntu image, the agents' tools and the container images, so
it takes a while. Running it again is safe: it keeps the answers, secrets and
work already on the VM.

## One fleet or several: shared IRC

Run one IRC server for the whole setup, however many fleets there are. The
user's answer to whether they intend to run more than one fleet decides
where that server runs and where its history is stored.

| | One fleet (or a local trial) | Several fleets, or may grow |
| --- | --- | --- |
| IRC (Ergo) | On the fleet VM | One shared server in a container for all fleets: [Azure Container Apps](#example-azure-container-apps) or another container host, or a dedicated VM the installer sets up (`./install.sh irc-server`) |
| IRC history | PostgreSQL, always (unless you pass `--no-api`, which leaves SQLite): the same instance as the API, in its own `ergo_history` database with its own role | PostgreSQL by default; SQLite is fine on a dedicated IRC VM if the user prefers |
| Subspace API | On the fleet VM, in the same PostgreSQL (`subspace` database) | See below |

- **One fleet.** Simplest. One PostgreSQL holds two databases, `subspace`
  for the API and `ergo_history` for Ergo, with separate roles and generated
  passwords, so there is a single database service to back up. The desktop
  reaches everything through the same tunnel. This is the installer's
  default, and the right choice for a local trial or a fleet that will stay
  the only one.
- **Several fleets.** One Ergo server for all your fleets. Each fleet keeps
  its own standard channels, named after the fleet, and its agents
  have their own accounts. The desktop stays connected to that one network.
  When you change fleet, by picking an initiative of another fleet in
  **Channels**, it loads that fleet's channels, so you work across fleets while
  seeing only the channels and captains of the fleet you are in. Stargate on
  each VM ties the channels to that fleet's agents. Choose this when the user
  plans more than one fleet VM, or already runs IRC elsewhere, for example as
  a container app.

The desktop app talks to one Subspace API, so with several fleets the API
belongs on a shared host too, ideally the same one as IRC. Today the installer
runs the API on the first fleet's VM. Install further fleets with
`--api-target <first fleet's target>`: they skip their own API, and the
installer registers each one on the first fleet's API: the fleet with its
standard channels, a ship for each captain, the rear admiral's and captains'
agent records and an initiative for it. That uses the first fleet's development sign-in, which the
installer checks before changing anything; once that API uses WorkOS or Entra,
register further fleets through its fleet, ship, agent and initiative
endpoints as an administrator instead, with the records described in the
[installer README](subspace-installer/README.md#subspace-api-registration). A further fleet's own agents get no agent listener today, so they have no
Subspace MCP server: the listener binds the first fleet VM's loopback, and the
installer does not yet carry it across the private network (for example with
an SSH forward from each further fleet VM to the first VM's port 3001, run
as an operator-owned service). Further fleets need the shared IRC server,
because IRC on their own VM is unreachable from the first fleet's API.
`--api-target` works only with a shared server the installer manages
(`--irc-server`); with one in a container, register further fleets as
[Point the fleets and desktops at it](#point-the-fleets-and-desktops-at-it)
shows. The desktop needs the first fleet's tunnel, which carries the API.
Every fleet's tunnel uses the same local ports for its dashboard, AgentsView
and Fleet API, so to look at another fleet's dashboard, stop the first fleet's
tunnel and start that fleet's for a while. For the same reason the API records
the tunnel's Fleet API address, `http://127.0.0.1:8484`, for the first fleet
only. Give a further fleet an HTTPS address with `--fleet-api-url` for the
desktop to show its live state.

The shared server and every fleet must reach each other over a private
network such as a tailnet or a cloud virtual network, and the desktop must be
on it too. The server needs a DNS name that matches its TLS certificate.
There are two ways to run it:

- **In a container on a container platform**, such as Azure Container Apps.
  This is what we run. You set it up with the platform's own CLI by following
  [Run the shared IRC server in a container](#run-the-shared-irc-server-in-a-container);
  the installer does not deploy to container platforms.
- **On a dedicated VM that the installer manages**, on any cloud. It runs
  the same Ergo container under Docker Compose, adds each fleet's accounts
  for you and fills in the desktop's IRC settings. See below.

### Shared IRC on a dedicated VM

Create the VM as in [Create a cloud VM](#create-a-cloud-vm), with a
Tailscale MagicDNS name, which works well as the server name. Set the server
up once, then point each fleet at it:

```sh
./install.sh irc-server --remote ops@irc.example.ts.net --host-key SHA256:... --yes \
  --server-name irc.example.ts.net --admin admiral
./install.sh --remote ops@fleet-one.example.ts.net --host-key SHA256:... --yes \
  --irc-server subspace-irc --name "Fleet One" --rear-admiral Atlas --captains Orion
./install.sh --remote ops@fleet-two.example.ts.net --host-key SHA256:... --yes --target fleet-two \
  --irc-server subspace-irc --api-target subspace-fleet --name "Fleet Two" --rear-admiral Nova --captains Lyra
```

`--bind` picks the private address for the TLS listener (default: the
Tailscale address, else the first private address). `--history` picks where
Ergo keeps history: `postgresql` (the default) runs a PostgreSQL container
beside Ergo on that host; `sqlite` keeps it in Ergo's data volume. Without `--tls-cert` and
`--tls-key`, Ergo makes a self-signed certificate, and the installer copies it
to each fleet and to the desktop's IRC settings. With Tailscale, `tailscale
cert` gives a trusted certificate for the MagicDNS name instead. Adding a
fleet restarts Ergo for a moment; agents reconnect on their own.

An IRC server the installer does not manage works too, such as one in a
container; see the next section.

## Run the shared IRC server in a container

This runs one Ergo server for every fleet and desktop on a container
platform. The repository provides the image's source
([`subspace-irc/`](subspace-irc/README.md)) and, on each fleet VM, the
`--irc remote` mode and the accounts that fleet needs. It does not build or
publish an image, and it does not deploy to container platforms: you do that
with the platform's official CLI by following this section, as for
[Create a cloud VM](#create-a-cloud-vm), and the same rules apply. Use only
the subscription, region, network and cost the user accepted, under their
existing sign-in; create no credentials (service principals, access keys or
API tokens) and change no account-level settings; tag and record everything
you create; and delete nothing without asking.

### Rules for any container host

- **Image.** Build it from `subspace-irc/`, whose Dockerfile pins Ergo by
  digest, and push it to a private registry in the user's account, tagged
  with the commit you built (`git rev-parse --short=12 HEAD`). There is no
  published image to pull.
- **One replica.** Run exactly one instance, with its data volume at `/ircd`.
  That volume holds the accounts and channel registrations (`ircd.db`), any
  SQLite history and a generated certificate if you supply none. The image
  locks `/ircd/instance.lock`, so a second instance on the same volume stops
  with "The IRC data volume is already in use".
- **History in PostgreSQL.** On a platform whose persistent volumes are
  network file shares, keep history out of SQLite, which SQLite's authors
  advise against running on network file systems: set
  `IRC_HISTORY_DATABASE=postgresql` and point Ergo at a private PostgreSQL
  database (`IRC_POSTGRES_HOST`, `IRC_POSTGRES_USER`,
  `IRC_POSTGRES_DATABASE`, `IRC_POSTGRES_SSL_MODE=require` or stricter,
  `IRC_POSTGRES_PASSWORD_FILE`). Ergo creates its own tables.
- **One listener, raw TCP, TLS by Ergo.** Fleets and desktops use the TLS
  listener, port `6697`; `--irc remote` always uses TLS. Turn the others off
  (`IRC_ENABLE_PLAIN=false`, `IRC_ENABLE_WS=false`). Publish `6697` as plain
  TCP, never through an HTTP ingress, and let Ergo terminate TLS.
- **A private DNS name and its certificate.** Pick the name fleets and
  desktops will use (here `irc.example.com`), resolvable only on the private
  network, and set `IRC_SERVER_NAME` to it. Supply the certificate and key
  yourself (`IRC_TLS_CERT` and `IRC_TLS_KEY`, which may name the same PEM
  file) rather than relying on the one Ergo generates. A self-signed one is
  fine; renew it before it expires, then restart the server and give fleets
  and desktops the new copy:

  ```sh
  umask 077
  openssl req -x509 -newkey rsa:2048 -nodes -days 365 -sha256 \
    -subj "/CN=irc.example.com" -addext "subjectAltName=DNS:irc.example.com" \
    -keyout irc-tls.key -out irc-ca.pem
  cat irc-ca.pem irc-tls.key > irc-tls.pem && rm irc-tls.key
  ```

  `irc-ca.pem` is the public half that fleets (`--irc-ca`) and desktops (the
  network's **CA certificate**) trust.
- **Secrets as mounted files.** Generate the operator password and a
  password for each human account (`openssl rand -hex 24`), and the history
  database password, into owner-only files on this computer. Give them, the
  TLS bundle and the account and channel seeds to the platform's secret
  store, and mount them as files for the `*_FILE` variables, readable by UID
  10001. Never put them in plain environment variables, print them or leave
  copies behind. Use `ircop` as the operator name (`IRC_OPER_NAME`), as the
  installer-managed server does.
- **Private only.** No public address, and ingress only from the fleet VMs
  and the desktops' private network. Every fleet VM and every desktop must
  resolve the name to the server's private address and route to it.

### Seed the accounts and channels

The server's accounts are the humans' accounts (at least the user's own,
from question 6) plus every fleet's agents; its channels are every fleet's
channels. Each fleet needs at least its
[standard channels](subspace-installer/README.md#channels):
`#<fleet>-starbase`, `#<fleet>-command` and `#<fleet>-<captain>` for each
captain, as in `#fleet-one-starbase`, `#fleet-one-command` and
`#fleet-one-orion`. Add any initiative channels beside them. Each fleet VM
prints what it needs as an IRC bundle (see the next section), with its
standard channels already in it. Keep the bundles in an owner-only folder on this computer, because
they hold the agents' IRC passwords, and build the seeds from them:

```sh
umask 077
jq -n --rawfile pw admiral.password '[{name: "admiral", password: ($pw | rtrimstr("\n"))}]' > humans.json
jq -cs '.[0] + [.[1:][].accounts[]]' humans.json bundles/*.json > accounts.json
jq -cs '[.[].channels[]]' bundles/*.json > channels.json
```

Before the first fleet, the seeds are `humans.json` and `[]`. Ergo reads them
at start, adds missing accounts, checks existing ones and registers missing
channels with `ircop` as founder ([Subspace IRC](subspace-irc/README.md#accounts-and-channel-access)).
To add a fleet, rebuild the seeds, update the two secrets and restart the
server; agents reconnect on their own. Removing a fleet's entries leaves its
accounts in Ergo.

### Point the fleets and desktops at it

1. **Each fleet.** Copy `irc-ca.pem` to the fleet VM first (for example
   `scp irc-ca.pem azureuser@fleet-one.example.ts.net:`) and install with
   `--irc remote --irc-host irc.example.com --irc-ca /home/azureuser/irc-ca.pem --irc-auth password`
   (add `--irc-port` for a port other than 6697): the seeds give each officer
   a password. Then fetch its bundle over
   the same SSH route the installer uses, add it to the seeds as above and
   restart the server:

   ```sh
   ssh azureuser@fleet-one.example.ts.net sudo -n subspace-setup irc-bundle > bundles/fleet-one.json
   ```

2. **Further fleets.** `--api-target` refuses a server the installer does not
   manage, so install each further fleet with `--no-api` and the same
   `--irc remote` options and its own `--target`, then register it with the
   first fleet's API. The registration holds no secrets: the fleet's IRC
   server, its standard channels, each captain's ship and channel, each
   officer's name, IRC nick, model and account subject, and the fleet's
   Stargate Fleet API address if you gave it one with `--fleet-api-url` (its
   tunnel's loopback address is the first fleet's, so it is never sent).

   ```sh
   ssh azureuser@fleet-two.example.ts.net sudo -n subspace-setup fleet-registration > fleet-two.json
   ssh azureuser@fleet-one.example.ts.net sudo -n subspace-setup register-fleet - < fleet-two.json
   ```

   That uses the first fleet's development sign-in, as `--api-target` does;
   `register-fleet --check` tests it first without changing anything.
3. **Each desktop.** The installer fills in the desktop's IRC settings only
   for a server it manages, so add the network under **Settings > Channels >
   Add network**: host `irc.example.com`, port `6697`, TLS on, the path to
   `irc-ca.pem` as **CA certificate**, and the user's account as nickname and
   SASL username with its password from the owner-only file. The desktop must
   resolve and reach that name over the private network (see the example
   below).

### When the simpler choice is right

For one fleet that will stay the only one, or for a local trial, keep Ergo on
the fleet VM: it is the installer's default and needs none of this. When a
container platform cannot meet the rules above, use the installer-managed
[dedicated VM](#shared-irc-on-a-dedicated-vm) instead: it runs the same
container on the VM's own disk.

### Example: Azure Container Apps

This is the arrangement we run, written as an example to adapt: the
commands have not been run as written here, so check each against
`az <command> --help` and stop on the first error. Ask the user for the
subscription, region and virtual network (the fleet VMs' network, so they
reach the server directly), or permission to create one, and confirm the
cost first: a Container Apps environment and app, a container registry, a
storage account, an Azure Database for PostgreSQL flexible server and, unless
you pass `--logs-destination`, the Log Analytics workspace the environment
creates. What the platform does and does not give the server:

- **Private TCP ingress.** Use an *internal* environment
  (`--internal-only true`) on a subnet of the user's virtual network: it has
  no public endpoint. Give the app TCP ingress (`transport: tcp`), which
  needs such a virtual network, with `external: true`. On an internal
  environment, *external* means reachable from the virtual network through
  the environment's internal load balancer; *internal* would limit the app to
  other apps in the same environment, so the fleets could not reach it.
- **Persistent storage.** `/ircd` is an Azure Files (SMB) share, so history
  goes in a PostgreSQL flexible server with private access in the same
  network. Whether the image's lock on `/ircd/instance.lock` holds across an
  SMB mount has not been verified. Container Apps starts a new revision
  before it stops the old one, so if Ergo fails to start on the share, or
  two replicas ever run at once, use the [dedicated VM](#shared-irc-on-a-dedicated-vm)
  instead.
- **Reaching it.** Fleet VMs in the same virtual network resolve the name
  through a private DNS zone linked to it. Desktops need a route to the
  environment's static IP and a DNS answer for the name: on Tailscale, a
  subnet router in that network (a small VM of its own, not a fleet VM)
  advertising the IP as a `/32`, which the user approves in the admin
  console and allows on port 6697 in the tailnet policy; on Cloudflare Zero
  Trust, a tunnel route to it for WARP clients. Resolve the name with split
  DNS, a resolver policy or a hosts-file entry.

Create the network pieces, the environment and the private DNS record:

```sh
az group create --name subspace-irc-rg --location <region> --tags app=subspace
az network vnet subnet create --resource-group <vnet-rg> --vnet-name <vnet> \
  --name subspace-irc --address-prefixes 10.0.8.0/27 \
  --delegations Microsoft.App/environments --service-endpoints Microsoft.Storage
az containerapp env create --resource-group subspace-irc-rg --name subspace-irc-env \
  --location <region> --internal-only true --tags app=subspace \
  --infrastructure-subnet-resource-id <subnet-id>
ip=$(az containerapp env show -g subspace-irc-rg -n subspace-irc-env --query properties.staticIp -o tsv)
az network private-dns zone create -g subspace-irc-rg -n example.com
az network private-dns link vnet create -g subspace-irc-rg -z example.com -n subspace \
  --virtual-network <vnet-id> --registration-enabled false
az network private-dns record-set a add-record -g subspace-irc-rg -z example.com -n irc -a "$ip"
```

Build and push the image, and give the app an identity that may only pull
it. The registry's endpoint needs an Entra sign-in to pull, and the image
holds no secrets:

```sh
az acr create -g subspace-irc-rg -n <registry> --sku Basic --tags app=subspace
az acr login -n <registry>
tag=$(git rev-parse --short=12 HEAD)
docker build --platform linux/amd64 -t "<registry>.azurecr.io/subspace-irc:$tag" subspace-irc
docker push "<registry>.azurecr.io/subspace-irc:$tag"
az identity create -g subspace-irc-rg -n subspace-irc-pull --tags app=subspace
az role assignment create --assignee-object-id <identity-principal-id> \
  --assignee-principal-type ServicePrincipal --role AcrPull --scope <registry-id>
```

Create the share for `/ircd`, open to the environment's subnet only, and the
history database. This server holds nothing but IRC history, so Ergo signs
in with its administrator login, `ergo`; on a shared server, create a
dedicated role instead:

```sh
az storage account create -g subspace-irc-rg -n <storage> --location <region> \
  --sku Standard_LRS --kind StorageV2 --min-tls-version TLS1_2 \
  --allow-blob-public-access false --default-action Deny --tags app=subspace
az storage account network-rule add -g subspace-irc-rg --account-name <storage> --subnet <subnet-id>
az storage share-rm create -g subspace-irc-rg --storage-account <storage> --name irc-data --quota 5
az containerapp env storage set -g subspace-irc-rg -n subspace-irc-env --storage-name irc-data \
  --storage-type AzureFile --azure-file-account-name <storage> --azure-file-share-name irc-data \
  --azure-file-account-key "$(az storage account keys list -g subspace-irc-rg -n <storage> --query '[0].value' -o tsv)" \
  --access-mode ReadWrite
az postgres flexible-server create -g subspace-irc-rg -n <pg-server> --location <region> \
  --vnet <vnet-id> --subnet <pg-subnet-id> \
  --private-dns-zone <pg-server>.private.postgres.database.azure.com \
  --tier Burstable --sku-name Standard_B1ms --storage-size 32 --version 17 \
  --admin-user ergo --admin-password "$(cat ergo-history.password)" \
  --database-name ergo_history --tags app=subspace --yes
```

The PostgreSQL subnet is a separate one, delegated to
`Microsoft.DBforPostgreSQL/flexibleServers`. Then write the app's
specification as an owner-only file, `subspace-irc.yaml`, filling each secret
from its file without printing it, create the app and delete the file:

```yaml
location: <region>
identity:
  type: UserAssigned
  userAssignedIdentities:
    <identity-id>: {}
properties:
  managedEnvironmentId: <environment-id>
  configuration:
    activeRevisionsMode: Single
    ingress:
      external: true  # internal environment: the virtual network only
      transport: tcp
      targetPort: 6697
      exposedPort: 6697
    registries:
      - server: <registry>.azurecr.io
        identity: <identity-id>
    secrets:
      - { name: irc-tls, value: <irc-tls.pem> }
      - { name: irc-operator-password, value: <operator password> }
      - { name: irc-accounts, value: <accounts.json> }
      - { name: irc-channels, value: <channels.json> }
      - { name: irc-history-password, value: <ergo-history.password> }
  template:
    containers:
      - name: irc
        image: <registry>.azurecr.io/subspace-irc:<tag>
        resources: { cpu: 0.5, memory: 1Gi }
        env:
          - { name: IRC_SERVER_NAME, value: irc.example.com }
          - { name: IRC_ENABLE_PLAIN, value: "false" }
          - { name: IRC_ENABLE_WS, value: "false" }
          - { name: IRC_TLS_CERT, value: /run/subspace/irc-tls }
          - { name: IRC_TLS_KEY, value: /run/subspace/irc-tls }
          - { name: IRC_OPER_NAME, value: ircop }
          - { name: IRC_OPER_PASSWORD_FILE, value: /run/subspace/irc-operator-password }
          - { name: IRC_ACCOUNTS_FILE, value: /run/subspace/irc-accounts }
          - { name: IRC_CHANNELS_FILE, value: /run/subspace/irc-channels }
          - { name: IRC_HISTORY_DATABASE, value: postgresql }
          - { name: IRC_POSTGRES_HOST, value: <pg-server>.postgres.database.azure.com }
          - { name: IRC_POSTGRES_SSL_MODE, value: require }
          - { name: IRC_POSTGRES_PASSWORD_FILE, value: /run/subspace/irc-history-password }
        volumeMounts:
          - { volumeName: secrets, mountPath: /run/subspace }
          - { volumeName: data, mountPath: /ircd }
    scale: { minReplicas: 1, maxReplicas: 1 }
    volumes:
      - { name: secrets, storageType: Secret }
      - name: data
        storageType: AzureFile
        storageName: irc-data
        mountOptions: uid=10001,gid=10001,dir_mode=0700,file_mode=0600
```

```sh
az containerapp create -g subspace-irc-rg -n subspace-irc --yaml subspace-irc.yaml
rm subspace-irc.yaml
az containerapp logs show -g subspace-irc-rg -n subspace-irc --tail 50
```

Each secret is mounted as a file named after it. To add a fleet, rebuild the
seeds, then update the two secrets and restart the revision:

```sh
az containerapp secret set -g subspace-irc-rg -n subspace-irc \
  --secrets "irc-accounts=$(cat accounts.json)" "irc-channels=$(cat channels.json)"
az containerapp revision restart -g subspace-irc-rg -n subspace-irc \
  --revision "$(az containerapp show -g subspace-irc-rg -n subspace-irc --query properties.latestRevisionName -o tsv)"
```

`az group delete --name subspace-irc-rg` deletes all of this except the
subnets in the user's virtual network. Run it only when the user asks.

## What the installer does

1. Creates the local VM, or checks the remote VM's host key and sudo access.
   It does not create cloud VMs: you create those first, as in
   [Create a cloud VM](#create-a-cloud-vm).
2. Copies `subspace-installer/`, `subspace-api/` and `subspace-irc/` to
   `/opt/subspace/bootstrap` on the VM.
3. Runs `subspace-setup` there, which:
   - writes the fleet's settings to `/root/fleet/` and generates every
     password and key on the VM;
   - before any service starts, keeps the agents' Linux users away from the
     API's main port, PostgreSQL and the cloud metadata service (agent
     isolation, which never blocks your own login and is never rolled back),
     and lets only the officers' users reach the API's agent listener;
   - runs the [fleet installer](subspace-installer/README.md): the latest
     Stargate (its newest `vX.Y.Z` release, looked up on every run and pinned
     in `fleet.json`; pass `--stargate-ref <SHA>` to stay on one commit),
     CLIProxyAPI, OpenCode, AgentsView, the dashboard, a Linux user and tmux
     session for the rear admiral and each captain, the crew's worktrees and
     each captain's pinned session, and Ergo IRC;
   - starts PostgreSQL and the Subspace API on loopback, creates IRC's
     history database (`ergo_history`) beside the API's, signs you in first
     (the first user in an organisation becomes its administrator), seeds the
     demo data if asked, creates an initiative for the fleet and registers
     the fleet with its standard channels, a ship for each captain and the
     rear admiral's and captains' agent records, which the desktop uses to
     join the channels and mark who is the rear admiral and who is a captain;
   - turns on Stargate's Fleet API when the installed Stargate has it;
   - with agent dispatch on, gives the rear admiral's gateway a token for the
     rear admiral's own agent record, gives Stargate's gateway user, and no
     agent, access to the API's port, and switches dispatch on in the rear
     admiral's gateway;
   - applies the lockdown and runs the privacy audit.
4. Confirms the lockdown over a fresh SSH connection, then runs the audit
   again. If that connection fails, the change rolls itself back after three
   minutes to the last confirmed lockdown, and the installer stops with an
   error. It also stops if the audit fails. It only reports success when the
   lockdown is confirmed and the audit passes. A re-run that changes nothing
   leaves the confirmed lockdown as it is.
5. Saves your IRC password and the Stargate Fleet API token as owner-only
   files under `~/.subspace/targets/<target>/`, and adds the fleet's IRC
   connection to the desktop app's settings.

## Bring the agents online

The agents get their models from [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)
on the VM. **Until it is signed in to at least one of your agentic provider
accounts, the agents are offline**: they join IRC but cannot think. Sign in
each account the fleet should use, from the repository on your computer:

```sh
./install.sh providers                        # which accounts are signed in
./install.sh providers login codex-device     # ChatGPT / Codex, with a device code
./install.sh providers login claude           # Claude
```

On Windows, use `.\install.ps1 providers ...` the same way. Each login prints
a link: open it in your browser and sign in with the account the agents should
use. `codex-device` shows a code to enter and needs nothing else. `claude` and
`codex` call back to your computer on port 54545 or 1455; the installer
forwards that port to the VM for the login. The proxy restarts afterwards and
the agents come online. CLIProxyAPI also supports `antigravity`, `kimi`,
`kimi-ai`, `xai`, `devin` and `meta` accounts.

The default models are `gpt-5.4` for Codex-family agents and
`claude-sonnet-5-5` for Claude-family crew. Use model IDs your accounts can
reach. Set the crew's with `sudo subspace-setup --crew-claude-model MODEL
--crew-codex-model MODEL` (or the same options on `./install.sh`); setup
keeps them on later runs, and `''` returns to the default. Change the rear
admiral's and captains' model under `harness` in `/root/fleet/fleet.json`
and run `sudo subspace-setup`, which keeps it.
The [fleet installer's guide](subspace-installer/README.md#model-provider-sign-in)
covers models and harnesses. On the VM, `sudo subspace-setup providers`
does the same as `./install.sh providers`.

## Connect the desktop app

1. **Start the tunnel.** It forwards the fleet's ports to `127.0.0.1` on your
   computer: the API (3000), IRC (6667 and 6697), the dashboard (8070),
   AgentsView (8080) and the Stargate Fleet API (8484) when it is on.

   ```sh
   ./install.sh tunnel --background
   ```

   Stop the local stack first if it is running (`make stack-down`), because it
   uses the same ports.
2. **Run the desktop app.** From the repository root, `make run` (see the
   [README](README.md#run-the-whole-system-locally) for the toolchain). Its
   default build signs in to `http://127.0.0.1:3000` as `developer@example.com`
   in organisation `local`, which is the administrator the installer created.
   If you chose another `--admin-email` or `--organisation`, set
   `SUBSPACE_DEV_EMAIL` and `SUBSPACE_DEV_ORGANISATION` for the app (on macOS,
   in a build configuration; see the [desktop README](subspace-desktop/README.md)).
   Release downloads may be built for another sign-in provider, so build from
   source for this setup.
3. **Channels.** The installer already added the fleet's IRC network, signed in
   as your account with the password file it saved. Pick the fleet's
   initiative in **Channels** to join its Starbase, fleet command and ship
   channels. Say hello to the rear admiral in Starbase. If the desktop's settings file could not
   be updated automatically, the installer printed the network to add under
   **Settings > Channels > Add network**; `./install.sh desktop` writes it
   again.
4. **Fleet Console: connect Subspace to Stargate's API.** The installer
   installs the latest Stargate release (v0.40.1 or later), turns on its
   private Fleet API on the VM's loopback port 8484, and saves its token in
   `~/.subspace/targets/<target>/stargate-api.token`. The tunnel forwards the
   port, so the **Stargate API address** is `http://127.0.0.1:8484`. The
   installer records that address in the Subspace API with the fleet, which
   is where the desktop app learns it; the desktop app asks for the fleet's
   token once and keeps it on your computer. The token is never stored in the
   Subspace API.
   - Fleet Console is built into Subspace on macOS and Windows, with its
     permissions allowed, so there is nothing to install. On macOS the
     installer fills in its Stargate API address and token, even before the
     app first starts. If Subspace was running, quit it and run
     `./install.sh desktop` again. On Windows, start Subspace once, quit it
     and run `.\install.ps1 desktop`. Or enter both yourself in
     **Settings > Spaces > Fleet Console**, pasting the token from the file
     above. For a copy you installed under another Board ID, add
     `--fleet-console <id>`.
   - Through Tailscale or Cloudflare instead of the tunnel, use that HTTPS
     address, and record it with the fleet by running the install command
     again with `--fleet-api-url https://fleet.example.ts.net:8484` (your own
     address; `--fleet-api-url tunnel` goes back to the tunnel's). The token is
     still required, and stays on your computer.

Open the dashboard at <http://127.0.0.1:8070> and AgentsView at
<http://127.0.0.1:8080> while the tunnel runs. They show prompts and code, so
keep them private.

## Use your local agents with the fleet

Your own coding agents can work with the fleet too: read and update issues,
log decisions, write documents, and talk to the rear admiral and captains on
IRC. The [Subspace skills](agent-skills/README.md) add this to Claude Code,
Codex and OpenCode, each in its own form, and register the Subspace API's MCP
server with them:

```sh
./install.sh skills --agents claude,codex,opencode --name "Your Name"
```

On Windows, use `.\install.ps1 skills ...`. This changes your agents' user
profiles, so agents run it only when the user agreed to it in the plan. Local agents sign in with
**your** login. They must always say they are an agent acting for you, with
their model and version, for example
`[on behalf of George (Claude Opus 5.5)]` on IRC, or
`**On behalf of George (Claude Opus 5.5):**` at the start of a tracker
comment. The skills require this, and the IRC helper adds it to every line.
The fleet's agents take orders from your account, so local agents send only
what you asked for.

## Check that the fleet is private

```sh
./install.sh audit
```

The audit fails if any service listens beyond loopback (apart from SSH and a
VPN agent), if the lockdown firewall is not loaded, if SSH accepts passwords or
root logins, if a container publishes a port publicly, or if secrets are
readable by anyone but root. It warns when the VM has a public address: then
check from outside that nothing answers, for example `nc -vz <public-ip> 22`
should time out unless you chose public SSH.

### Who can reach the API

Development sign-in, which the desktop uses by default, trusts anyone who can
reach the API's main port (3000) to act as anyone, administrator included.
The lockdown keeps the agents' users away from that port, and the tunnel
keeps everyone else away. For a team, switch the API and desktop to
WorkOS or Entra (see the [API guide](subspace-api/README.md#external-sign-in))
before sharing access.

The fleet's agents reach the API on its **agent listener** instead: port 3001
on the VM's loopback, which the tunnel does not forward. There, a request
names a registered agent in an `X-Subspace-Agent` header and needs no token.
The API resolves only an active agent record of the fleet's organisation and
acts as that agent, which is never an administrator; the listener serves no
sign-in route and no administrator route or tool. It refuses any request from a
browser page (one with an `Origin` header) and any request addressed to a host
other than `127.0.0.1`, `localhost` or `[::1]`. The lockdown lets only root,
your operator login and the officers' own Linux users (the rear admiral's and
each captain's; crew run as their captain) reach port 3001, not the fleet's
other service users or containers. Any of those can act as any registered
agent of that organisation, so one agent can act as another.
`./install.sh audit` checks that port 3001 is limited that way and serves no
sign-in route, and that the main port stays closed to the agents. To turn
the listener off, set `AGENT_PORT=` in `/root/fleet/api.env` and run setup
again; later runs keep it off, and the agents then have no Subspace MCP
server. Switching the API to WorkOS or
Entra (development sign-in off) turns the agent listener off too, and the
fleet's agents then have no way into the API yet.

## Day to day

| Task | Command |
| --- | --- |
| See or add provider accounts (agents need one) | `./install.sh providers`, `./install.sh providers login codex-device` |
| Open a shell on the VM | `./install.sh ssh` |
| Watch a captain work | `./install.sh ssh`, then `sudo subspace-fleetctl attach orion` |
| Follow Stargate's logs | `sudo journalctl --namespace=subspace-fleet -u subspace-fleet-gateway.service -f` |
| Show the fleet and services | `./install.sh status` |
| Change a setting | Run the install command again with the new flags |
| Connect the rear admiral to Slack or Teams | [Slack](docs/slack.md), [Teams](docs/teams.md) |
| Change the repositories | Run the install command again with `--repositories`; it replaces the list. Once a captain's session is pinned, the installer refuses an empty list or a different first repository, whether reordered or replaced |
| Add private repositories or GitHub Apps | Edit `/root/fleet/fleet.json` on the VM, put credential files under `/root/fleet/fleet.secrets/`, then `sudo subspace-setup` |
| Print your IRC password | `sudo subspace-setup secret irc-admin` on the VM |
| Seed the demo data again | `sudo subspace-setup demo` on the VM |
| Stop or start a local VM | `./install.sh stop`, `./install.sh start` |
| Delete a local VM | `./install.sh destroy` |

`--target NAME` picks which VM a command acts on when you have several
(default `subspace-fleet`). See the [fleet installer's guide](subspace-installer/README.md)
for repositories, GitHub Apps for the rear admiral and captains, crew,
permissions, Slack and Teams.

## Secrets

| Secret | Where it lives |
| --- | --- |
| IRC passwords, model proxy key, PostgreSQL password | Generated on the VM in `/root/fleet/` (root-only), optionally also in Key Vault |
| Installed copies the services read | `/etc/subspace-fleet/` (root-only), and each agent's files in its own home with mode 0600 |
| An existing PostgreSQL's URL and CA | `/root/fleet/database-url` and `database-ca.pem` (root-only), copied from your files on each run that passes them |
| Officers' IRC certificates (remote IRC) | `/root/fleet/fleet.secrets/<officer-id>/irc-client.pem` (root-only), generated on the VM; only fingerprints are printed |
| An IRC operator's password, and its sign-in account's | Your files only: copied to the VM for the run that passes them, then removed; never printed or saved |
| Model provider logins | `/var/lib/subspace-fleet/proxy/auth` on the VM, owned by the proxy user |
| Your IRC password and Stargate API token | `~/.subspace/targets/<target>/` on your computer, owner-only |
| A shared IRC server's secrets, in a container | Owner-only files on your computer and the platform's secret store ([Run the shared IRC server in a container](#run-the-shared-irc-server-in-a-container)) |
| Local VM SSH keys | Lima: `~/.lima/_config/`; Multipass on Windows: `~\.subspace\targets\<target>\` |

## If you get locked out

A lockdown change only becomes permanent after a new SSH connection
succeeds. If that fails, the change undoes itself after three minutes and the
last confirmed lockdown (or none, on a first install) applies again; wait and
try again. A restart also undoes an unconfirmed change. From a console:

```sh
sudo subspace-setup lockdown rollback
```

This undoes a pending change. With nothing pending, it removes the inbound
firewall and SSH hardening entirely, for example when a confirmed `--ssh-from`
no longer matches your address; run the installer again afterwards to restore
them. Agent isolation stays, because it never blocks your login.

- Local Lima VM: `limactl shell <target>` uses SSH too; if that fails, wait
  for the rollback, or stop and start the VM (`./install.sh stop`, then
  `start`), which undoes an unconfirmed change.
- Multipass: `multipass shell <target>`.
- Cloud VM: use the serial console or a run-command, for example on Azure
  `az vm run-command invoke … --scripts "subspace-setup lockdown rollback"`.

## Troubleshooting

- **The tunnel says a port is in use.** Something local already listens there,
  often the local stack: `make stack-down`.
- **The desktop shows no initiatives.** Check that the tunnel runs and that
  `curl http://127.0.0.1:3000/health` answers. Re-run the installer if the API
  was not up.
- **Channels names a server to add.** The desktop's IRC network must match the
  fleet's host, port and TLS setting exactly: `127.0.0.1`, `6667`, TLS off for
  IRC on the fleet VM. Run `./install.sh desktop`.
- **Agents do not answer.** Check `./install.sh providers`: with no signed-in
  account the agents are offline. Then watch the gateway's logs and check the
  model IDs in `/root/fleet/fleet.json`. Agents only take orders from your IRC account.
- **Lima or Multipass cannot start the VM.** On Linux, check that `/dev/kvm`
  exists and that you are in the `kvm` group. On Windows, enable Hyper-V;
  the installer cannot reach Multipass VMs on VirtualBox.
- **Read the full setup output again:** run the install command again; it is
  safe to repeat.

## Uninstall

On the VM, `sudo /opt/subspace/bootstrap/subspace-installer/install.sh uninstall`
stops the fleet and keeps its data. `sudo docker compose --project-name subspace-api down`
stops the API. For a local VM, `./install.sh destroy`
deletes everything. For a cloud VM, delete the resources recorded when it was
created (on Azure, its resource group) and remove it from the tailnet or
delete its Cloudflare tunnel; agents do this only when the user asks. Revoke any GitHub App keys and model logins you gave the
fleet.
