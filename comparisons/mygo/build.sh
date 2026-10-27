#!/bin/sh
# Builds the MyGo adapters into bin/ as MyGo apps ship (release: no race
# detector, no debug information, no cgo, as MyGo itself builds) and writes
# build.json, which the comparison runner copies into each run's audit.
#
#     cd comparisons/mygo && ./build.sh
set -eu
cd "$(dirname "$0")"

# The workload's Sinf ports macOS libm's sinf, so no cgo call is needed to
# match Rust's f32::sin (workload/sinf.go; checked by `go test ./workload
# -run SinfIsLibm -libm.all`).
export CGO_ENABLED=0
LDFLAGS="-s -w"
mkdir -p bin
for scene in heavy lab animate vlist micro blink field; do
	go build -trimpath -ldflags="$LDFLAGS" -o "bin/$scene" "./cmd/$scene"
done

mygo_version=$(go list -m -f '{{.Version}}' github.com/egoist/mygo)
mygo_commit=$(go mod download -json "github.com/egoist/mygo@$mygo_version" |
	sed -n 's/.*"Hash": "\([0-9a-f]*\)".*/\1/p')
cat >build.json <<EOF
{
  "mygo_version": "$mygo_version",
  "mygo_commit": "$mygo_commit",
  "mygo_license": "MIT",
  "mygo_source": "https://github.com/egoist/mygo",
  "go": "$(go version | sed 's/^go version //')",
  "flags": "-trimpath -ldflags=$LDFLAGS",
  "cgo": false,
  "sinf": "workload/sinf.go: macOS libm sinf ported to Go, bit-identical for all 2^32 inputs",
  "binaries": ["bin/heavy", "bin/lab", "bin/animate", "bin/vlist", "bin/micro", "bin/blink", "bin/field"],
  "built_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
echo "built bin/{heavy,lab,animate,vlist,micro} with MyGo $mygo_version ($mygo_commit)"
