# Regenerate with: cargo build --release && vhs assets/demo.tape
Output assets/demo.gif

Require git

Set Shell "bash"
Set FontSize 16
Set Width 1000
Set Height 520
Set Padding 16
Set TypingSpeed 80ms
Set Theme "Catppuccin Mocha"

Hide
Type "export PATH=$PWD/target/release:$PATH GIM_CONFIG=/dev/null && rm -rf /tmp/gim-demo && mkdir /tmp/gim-demo && cd /tmp/gim-demo && git init -q -b main && git commit -q --allow-empty -m init" Enter
Type "for b in feat/login feat/logout feat/search fix/typo docs/readme chore/deps wip/spike; do git branch $b; done && clear" Enter
Sleep 500ms
Show

Type "gim" Sleep 300ms Enter
Sleep 1s

# Move around
Type "jjj" Sleep 400ms
Type "kk" Sleep 800ms

# Fuzzy search
Type "/" Sleep 400ms
Type "log" Sleep 1s
Down Sleep 600ms
Up Sleep 600ms
Enter Sleep 800ms
Escape Sleep 1s

# Visual selection + delete
Type "g" Sleep 500ms
Type "v" Sleep 400ms
Type "j" Sleep 800ms
Type "d" Sleep 1s
Type "y" Sleep 1.2s

# Rename
Type "jjj" Sleep 600ms
Type "r" Sleep 500ms
Type "-v2" Sleep 500ms
Enter Sleep 1.2s

# Checkout via search
Type "/" Sleep 300ms
Type "srch" Sleep 800ms
Enter Sleep 400ms
Enter Sleep 1s
Type "git branch --show-current" Enter
Sleep 2s
