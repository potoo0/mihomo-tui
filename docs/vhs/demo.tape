# https://github.com/charmbracelet/vhs

# setup
Output out.mp4
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
