
## TEST TOOLS

Prerequisites:
$ sudo apt install curl
$ cargo install --features=ssl websocat

GET example:
$ curl -X GET "http://localhost:3030/boobs/1"

POST example:
$ curl -H "Content-Type: application/json" -d '{"name":"Roman","rate":5}' "http://localhost:3030/employees"

add -v to see error code

WS example connect to local server:
websocat ws://localhost:3030/echo
