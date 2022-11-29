# GDP Router 

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**

- [How to build](#how-to-build)
- [Testcases](#testcases)
- [TODOs](#todos)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->


### How to build 

```
cargo build
```

### Testcases 
1.Simple Forwarding

We can come up with the following test case: 
a router, a dtls client and a tcp client. We want to route tcp client's message
to dtls client. 
```bash
# (terminal A) run router
$ cargo run router

# (terminal B) run dtls client 
$ cargo run client

# (terminal C) run tcp client
$ nc localhost 9997
```

Then we can use the following sample test cases
```
# (dtls client) advertise itself with name 1
ADV,1
FWD,1,000 // this sends itself a message

# (tcp client) send message to name 1
FWD,1,111
FWD,1,222
FWD,1,333
FWD,1,444
```
We should expect messages appearing in dtls client's terminal.


2.Introducing Route Query

We can think of another test case: One domain (a.k.a remote) RIB, two routers, A and B, two TCP clients 1 and 2 connecting to the two routers separately. We want client 1's message to route to client 2 and vice versa. 
```bash
# (terminal A) run RIB
$ cargo run -- -c src/resources/test_rib_config.toml rib

# (terminal B) run router A
$ cargo run -- -c src/resources/test_router1_config.toml router

# (terminal C) run router B
$ cargo run -- -c src/resources/test_router2_config.toml router

# (terminal D) run tcp client 1
$ nc localhost 9997

# (terminal E) run tcp client 2
$ nc localhost 9998
```
Then we can use the following sample test cases
```
# (tcp client 1 on terminal D) advertise itself with name 3
ADV,3

# (tcp client 2 on terminal E) advertise ifself with name 4
ADV,4

# (tcp client 1) send a message to name 4
FWD,4,54321

# (tcp client 2) send a message to name 3
FWD,3,12345
```
We should see the message appearing in both tcp clients' terminals.



### TODOs
- [x] tcp test interface
- [x] dlts 
- [x] connection RIB 
- [ ] an actual future based RIB 
- [ ] zero copy multicast (can we adopt the design from pnet version?)
- [ ] grpc stream
- [ ] use gdp protocol (protobuf or just bytes?) 
- [ ] use name certificates instead of pseudo names  

minor 
- [ ] use app_config to config the ports and addresses 
- [ ] enhance error handling (e.g. connection is closed, packet wrong format)