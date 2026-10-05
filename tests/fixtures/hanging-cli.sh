#!/bin/sh
# Controlled nonresponsive RPC fixture. $3 is the isolated --data-dir argument.
printf '%s\n' "$$" > "$3/child.pid"
exec /bin/sleep 60
