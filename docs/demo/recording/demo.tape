# Run with docs/demo/generate.sh. All content comes from the local fixture.
Output ../demo.gif

Set Shell "bash"
Set Width 1500
Set Height 800
Set Padding 18
Set Framerate 12

Set Theme "Catppuccin Mocha"
Set FontSize 20
Set FontFamily "Sarasa Mono SC"
Set LetterSpacing 0

Set TypingSpeed 30ms
Set KeyboardOverlay Input
Set KeyboardOverlayLocation BottomRight

Require run-tui.sh

Type "run-tui.sh"
Enter
Sleep 1s
# Keep the rendered terminal state for diagnosing startup failures.
State ../../../target/demo-start.state.json
Screenshot ../../../target/demo-start.png
Wait+Screen "/Overview|View/"
Sleep 1s

# Connections: three local responses remain open while data arrives.
Type "2"
Wait+Screen "127.0.0.1"
Sleep 1s
Down
Enter
Wait+Screen "detail"
Sleep 2s
Escape
Type "s"
Wait+Screen "apply"
Sleep 2s
Escape

# Proxies and proxy groups.
Type "3"
Wait+Screen "Demo Route"
Sleep 2s
Down
Enter
Sleep 1s
Escape

# A provider served on 127.0.0.1. Its update stays local.
Type "4"
Wait+Screen "Demo Provider"
Sleep 1s
Down
Sleep 300ms
Type "u"
Sleep 1s
Enter
Sleep 2s
Escape

# Logs: ongoing local requests produce new routing events.
Type "5"
Sleep 2s

# Rules: show the local rule set and fallback rule.
Type "6"
Sleep 2s

# RuleProviders: the rule file is served by the local fixture server.
Type "7"
Wait+Screen "Demo Rules"
Sleep 2s

# Config: show core settings and open the DNS query window.
Type "8"
Sleep 1s
Type "n"
Sleep 3s
