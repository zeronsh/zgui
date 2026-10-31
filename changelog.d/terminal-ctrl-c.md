---
section: fixed
---
- **Ctrl+C reaches a terminal's program on Windows whatever started the app** (zgx-terminal): Windows passes a parent's choice to ignore Ctrl+C on to its children, so an app started by one (an agent's shell, a service, a launcher) started shells where Ctrl+C never interrupted anything. A session restores Ctrl+C handling before it starts its program.
