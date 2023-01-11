#!/bin/sh

set -e

cd "$(dirname "$0")"

jq -S < registry.json > registry.next.json
mv registry.next.json registry.json
