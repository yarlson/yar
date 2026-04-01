package main

fn worker(jobs chan[i32], results chan[i32]) void {
	for true {
		job := chan_recv(jobs) or |err| {
			break
		}
		chan_send(results, job * job) or |err| {
			break
		}
	}
}

fn main() i32 {
	jobs := chan_new[i32](4)
	results := chan_new[i32](4)

	taskgroup []void {
		spawn fn() void {
			chan_send(jobs, 2) or |err| {
				return
			}
			chan_send(jobs, 3) or |err| {
				return
			}
			chan_close(jobs)
		}()

		spawn fn() void {
			worker(jobs, results)
			chan_close(results)
		}()
	}

	total := 0
	for true {
		value := chan_recv(results) or |err| {
			break
		}
		total += value
	}

	print(to_str(total) + "\n")
	return 0
}
