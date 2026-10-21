#!/bin/sh
# One fixed sequence of requests against a running server; prints status line + body of each.
# Usage: ./smoke.sh http://127.0.0.1:3300      (compare the output of node and of the component)
B=${1:-http://127.0.0.1:3300}
r() { printf '%s %s\n' "$1" "$2"; shift; curl -s --noproxy '*' -i -H 'content-type: application/json' "$@" | sed -e '/^[Dd]ate:/d;/^[Cc]onnection:/d;/^[Kk]eep-[Aa]live:/d;/^[Ee][Tt]ag:/d;/^[Ll]ast-[Mm]odified:/d' -e 's/"created":"[^"]*"/"created":"<t>"/;s/"uptime":[0-9]*/"uptime":0/' | tr -d '\r'; echo; }
r GET /api/stats "$B/api/stats"
r POST /api/todos -X POST -d '{"title":"write the demo"}' "$B/api/todos"
r POST /api/todos -X POST -d '{"title":"post it"}' "$B/api/todos"
r POST /api/notes -X POST -d '{"title":"idea","text":"Express on WASI"}' "$B/api/notes"
r POST /api/notes -X POST -d '{"text":"  "}' "$B/api/notes"
r PATCH /api/todos/1 -X PATCH -d '{"done":true}' "$B/api/todos/1"
r GET /api/todos "$B/api/todos"
r DELETE /api/notes/4 -X DELETE "$B/api/notes/4"
r DELETE /api/notes/4 -X DELETE "$B/api/notes/4"
r GET /api/stats "$B/api/stats"
r GET /nothing "$B/nothing"
r GET "/api/todos?filter=open&q=demo" "$B/api/todos?filter=open&q=demo"
r GET "/api/notes?q=express" "$B/api/notes?q=express"
r GET / "$B/"
