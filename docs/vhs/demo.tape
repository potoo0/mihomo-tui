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
Sleep 1s

############################# tab=2: Connections ##########################
Type "2"
# enable capture mode
Type "c"
# search word `google`
Type@50ms "f'google"
Enter
Sleep 1s
# navigate to first item, view details
Down
Sleep 500ms
Enter
Sleep 1s
Escape
Sleep 300ms
# connection settings
Type "s"
Sleep 1500ms
Escape
Sleep 300ms

############################# tab=3: Proxies ##############################
Type "3"
Sleep 300ms
# update threshold settings
Type "s"
Sleep 300ms
Tab@300ms 2
Ctrl+W
Ctrl+W
Type@50ms "1000,2000"
Sleep 300ms
Enter
Sleep 300ms
# update selected proxy
Down@100ms 3
Enter
Sleep 500ms
Type "]"
Sleep 300ms
Down@100ms 2
Enter
Sleep 1s
Escape@300ms 2
Sleep 300ms
# test proxy
Type "t"
Sleep 1s

############################# tab=4: ProxyProviders #####################$#
Type "4"
Sleep 100ms
Space 2
Sleep 300ms
# update selected provider
Type "u"
Sleep 300ms
# view provider proxies
Enter
Sleep 2s
Escape
Sleep 300ms

############################# tab=5: Logs #################################
Type "5"
Sleep 300ms
Type "d"
Sleep 300ms
# search `go`
Type "fgo"
Sleep 3s
Enter

############################# tab=6: Rules #################################
Type "6"
Sleep 1200ms

############################# tab=7: RuleProviders #################################
Type "7"
Sleep 1200ms

############################# tab=8: Config #################################
Type "8"
Sleep 1s
# dns query
Type "n"
Sleep 300ms
Tab
Type@50ms "google.com"
Enter
Sleep 1500ms
