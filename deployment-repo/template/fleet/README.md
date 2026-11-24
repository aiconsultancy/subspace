# The fleet

What the fleet is, without its secrets:

- `fleet.json`: a copy of the fleet VM's `/root/fleet/fleet.json` (names,
  repositories, crew, models, harness, permissions). It holds no secrets;
  keep `fleet.secrets/` and every credential file out of this repository.
- `install.md`: the exact `./install.sh` commands this fleet was installed
  and last changed with, including `--stargate-ref` with the Stargate pin.
  Name secret files by their path on the operator's computer, never by
  content.

After a change on the VM, copy `fleet.json` back here and commit it with a
line saying why.
