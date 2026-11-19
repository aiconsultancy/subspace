# Intent: ACP agent marks and names

The owner asked:

> Can we include the ChatGPT, Claude and Opencode icons so that when the user
> selects ACP we can use that icon in the UI where we used to have the GitHub
> one yeah so then it can be codex ready, claude ready, open code ready

Before this change, every ACP agent shared the Lucide Plug glyph on the
toolbar's **Chat** button and in the chat header, where Copilot shows its own
mark. The status bar named the engine with a protocol suffix, such as "Codex
(ACP) ready", and showed no mark.

The goal is that the user can see at a glance which agent runs the chat, and
that the status bar reads "Codex ready", "Claude ready" or "OpenCode ready".
The owner's "ChatGPT" icon is OpenAI's mark, which identifies Codex.
