#!/bin/bash
# Clears Gatekeeper quarantine on ModDB.app, then launches it.
# Double-click this the first time after downloading the zip.
cd "$(dirname "$0")" || exit 1
xattr -dr com.apple.quarantine "ModDB.app" 2>/dev/null || true
open "ModDB.app"
