package main

import "env"
import "process"
import "stdio"

fn main() !i32 {
    args := process.args()
    if len(args) != 3 {
        return 1
    }

    value := env.lookup("YAR_PROCESS_ENV_TEST")?
    if value != "env ok" {
        return 1
    }

    capture := process.run([]str{args[1]})?
    if capture.exit_code != 7 {
        return 1
    }
    if capture.stdout != "captured stdout\n" {
        return 1
    }
    if capture.stderr != "captured stderr\n" {
        return 1
    }

    stdio.eprint("stdio stderr\n")

    inherit_code := process.run_inherit([]str{args[2]})?
    if inherit_code != 3 {
        return 1
    }

    print("process_env ok\n")
    return 0
}
