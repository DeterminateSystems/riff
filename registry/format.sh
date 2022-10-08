#!/bin/sh
set -exuo pipefail
cd "$(dirname "$0")"

jq -S < registry.json > registry.next.json
mv registry.next.json registry.json
