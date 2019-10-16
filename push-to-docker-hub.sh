#!/bin/sh
set -o errexit -o nounset -o pipefail
name=kierdavis/nix-precache:latest
docker build -t $name .
docker push $name
