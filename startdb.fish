#!/usr/bin/env fish

set -x DATABASE_URL "postgres://postgres@localhost:5432/stasher"
docker run --rm -p 5432:5432 -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=stasher postgres:latest
