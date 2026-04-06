# CRFR Demo: 10 Real Questions, 10 Live Websites
#
# Render:  vhs crfr-10questions.tape
# Output:  crfr-10questions.gif + crfr-10questions.mp4
# Length:  ~15 seconds
#
# Requires: vhs (https://github.com/charmbracelet/vhs)

Output crfr-10questions.gif
Output crfr-10questions.mp4

# ── Terminal look ──
Set Shell "bash"
Set FontFamily "JetBrains Mono"
Set FontSize 15
Set Width 1400
Set Height 780
Set Padding 20
Set TypingSpeed 30ms
Set Theme { "name": "CRFR", "black": "#0d1117", "red": "#ff7b72", "green": "#3fb950", "yellow": "#d29922", "blue": "#58a6ff", "magenta": "#bc8cff", "cyan": "#39d353", "white": "#c9d1d9", "brightBlack": "#484f58", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#e3b341", "brightBlue": "#79c0ff", "brightMagenta": "#d2a8ff", "brightCyan": "#56d364", "brightWhite": "#f0f6fc", "background": "#0d1117", "foreground": "#e6edf3", "selection": "#264f78", "cursor": "#e6edf3" }

# ── Act 1: Type the command (0.0s – 2.0s) ──
Sleep 400ms
Type "aether extract-batch --goals '10 real questions' --live"
Sleep 400ms
Enter

# ── Act 2: Stream script runs (2.0s – 13.0s) ──
# The typed command triggers crfr-10questions-stream.sh
# which has built-in sleep() for pacing each row.
# If running standalone, wire your .bashrc:
#   alias aether="bash demos/crfr-10questions-stream.sh"
# Or edit this tape to use:
#   Type "bash demos/crfr-10questions-stream.sh"

Sleep 12500ms

# ── Act 3: Summary holds on screen (13.0s – 15.5s) ──
Sleep 3000ms
