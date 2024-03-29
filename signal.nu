#!nu

def main [--build, ...args] {
    if $build {
        $args | to json | save --raw rebuild_signal -f
    } else {
        while true {
            print "Listening for rebuild signal"
            open --raw rebuild_signal | each {
                let args = ($in | from json)

                if ($args | is-empty) {
                    print "Received exit signal"
                    exit 0
                }

                clear
                print $"(ansi grey)$ (ansi white)cargo (ansi grey)run -- (ansi white_bold)($args | str join ' ')(ansi reset)\n"
                let build = (cargo run -- ...$args | complete)

                if $build.exit_code == 0 {
                    print $"\n(ansi green)Process exited successfully(ansi reset)"
                } else {
                    print $"\n(ansi red)Process did not exited successfully(ansi reset)"
                }
            }
        }
    }
}
