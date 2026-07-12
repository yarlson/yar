package main

import "std/env"
import "std/process"
import "std/stdio"

fn main() !i32 {
    args := process.args()
    if len(args) != 6 {
        return 1
    }

    value := env.lookup("YAR_PROCESS_ENV_TEST")?
    if value != "env ok" {
        return 1
    }

    limits := process.limits(5000, 1024, 1024)?
    cancellation := process.cancellation()
    capture := process.run([]str{args[1]}, limits, cancellation)?
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

    inherit_code := process.run_inherit([]str{args[2]}, 5000, cancellation)?
    if inherit_code != 3 {
        return 1
    }

    if expect_timeout(args[3])? != 0 {
        return 1
    }

    if expect_limit(args[4])? != 0 {
        return 1
    }

    cancel := process.cancellation()
    cancelled := taskgroup []!i32 {
        spawn wait_for_cancel(args[5], cancel)
        spawn trigger_cancel(cancel)
    }
    if cancelled[0]? != 0 || cancelled[1]? != 0 {
        return 1
    }

    print("process_env ok\n")
    return 0
}

fn expect_timeout(path str) !i32 {
    limits := process.limits(10, 1024, 1024)?
    process.run([]str{path}, limits, process.cancellation()) or |err| {
        if err == error.Timeout {
            return 0
        }
        return 1
    }
    return 1
}

fn expect_limit(path str) !i32 {
    limits := process.limits(5000, 3, 1024)?
    process.run([]str{path}, limits, process.cancellation()) or |err| {
        if err == error.LimitExceeded {
            return 0
        }
        return 1
    }
    return 1
}

fn wait_for_cancel(path str, cancellation process.Cancellation) !i32 {
    limits := process.limits(5000, 1024, 1024)?
    process.run([]str{path}, limits, cancellation) or |err| {
        if err == error.Cancelled {
            return 0
        }
        return 1
    }
    return 1
}

fn trigger_cancel(cancellation process.Cancellation) !i32 {
    process.cancel(cancellation)
    return 0
}
