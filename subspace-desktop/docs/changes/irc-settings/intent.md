# Intent: IRC settings

IRC networks could only be configured by editing `config.json` and
restarting, and the empty board explained that with a large JSON block. The
owner asked for three things:

- configure IRC from the Settings window;
- open the configuration folder from Settings in the system file manager;
- copy a prompt that lets a coding agent do the configuration.

The goal is that a person can add, change and remove networks without
touching JSON, that passwords keep the existing secret treatment (never shown,
never copied into an input, kept when untouched), that saved changes apply
without a restart where that is safe, and that an agent gets a complete,
accurate brief whose schema cannot drift from the code.
