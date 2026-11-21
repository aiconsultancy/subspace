# Boards and board guides

Use the board switcher to choose a board. Open **Commands > Help** for the
selected board's in-app guide. Press `?` while board navigation has focus as
an alternative. These links open the same source guides published in the app.

| Board | Guide |
| --- | --- |
| PRs | [GitHub pull requests](../../spaces/github/help.md) |
| Copilot Agents | [Copilot agent tasks](../../spaces/github-copilot-agents/help.md) |
| Channels | [IRC networks, channels and private messages](../../spaces/irc/help.md) |
| Issues | [Subspace API issues and comments](../../spaces/issues/help.md) |
| Decisions | [Subspace API decisions](../../spaces/decisions/help.md) |
| Space Lab | [Native dashboards and managed environments](../../spaces/space-lab/help.md) |
| Actions | [GitHub Actions](../../spaces/actions/help.md) |
| Component gallery | [Shared UI components for developers](https://github.com/aiconsultancy/subspace/blob/main/subspace-framework/spaces/component-gallery/help.md) |
| Web Spaces | [Websites and installed web Spaces](../../spaces/web/help.md) |

Space Lab and the Component gallery are not ready to ship, so released
builds of Subspace do not include them. They are in developer builds made
with their Cargo features (`make run-unreleased`), and start off there until
you turn them on in Settings or from **Add a Space**.

The terminal is not a board: debug builds dock it beside every board, as they
do the chat. Its guide is [Terminal](../../terminal/panel/help.md).

Each guide states its prerequisites, controls, actions, keyboard shortcuts,
settings, troubleshooting steps and limits. Read those prerequisites before
connecting a service. A successful sign-in does not grant service permissions.

Use [Settings](settings.md) to choose enabled boards and their order.
Right-click a board's tab and choose **Close Space** to turn it off without
opening Settings, or **Space settings…** to open its Settings page. To turn
a board back on, choose the plus after the last tab (**Add a Space**) and
pick it from the list. Some
boards are off by default, platform-specific or experimental. The
[developer overview](../../README.md) records those build differences.
