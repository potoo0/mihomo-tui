# https://github.com/charmbracelet/vhs

# setup
Output out.gif
Set TypingSpeed 0.1
Set Width 1500
Set Height 800
Set Padding 18
Set FontSize 18
Set FontFamily "Sarasa Mono SC"

# run program
Type@10ms "./mihomo-tui -c config.yaml"
Enter

############################# tab=1: Overview #############################
Sleep 3s

############################# tab=2: Connections ##########################
Type "2"
# enable capture mode
Type "c"
# search `google`
Type "fgoogle"
Enter
Sleep 1s
# sort by col 4 (Chains)
Right@500ms 4
Sleep 1s
# navigate to first item, view details
Down
Sleep 500ms
Enter
Sleep 1s
Space
Sleep 1s
Escape
Sleep 500ms

############################# tab=3: Proxies ##############################
Type "3"
Sleep 300ms
# update threshold settings
Type "s"
Sleep 300ms
Tab@300ms 2
Ctrl+W
Sleep 300ms
Ctrl+W
Sleep 300ms
Type "1000,2000"
Sleep 500ms
Enter
Sleep 1s
# update selected proxy
Right@100ms 11
Enter
Sleep 500ms
Right@300ms 2
Sleep 500ms
Enter
Sleep 1s
Escape@300ms 2
Sleep 300ms
# test proxy
Right
Sleep 500ms
Type "t"
Sleep 1s

############################# tab=4: ProxyProviders #####################$#
Type "4"
Sleep 100ms
Space
Sleep 300ms
# update selected provider
Type "u"
Sleep 1s
# view provider proxies
Enter
Sleep 2s
Escape
Sleep 300ms

############################# tab=5: Logs #################################
Type "5"
Sleep 300ms
Type "d"
Sleep 500ms
# search `go`
Type "fgo"
Sleep 3s
Enter

############################# tab=6: Rules #################################
Type "6"
Sleep 150ms
# navigate to first item, toggle disabled
Down
Sleep 300ms
Type "t"
Sleep 1500ms
Type "s"
Sleep 1500ms

############################# tab=7: RuleProviders #################################
Type "7"
Sleep 1500ms

############################# tab=8: Config #################################
Type "8"
Sleep 3s
