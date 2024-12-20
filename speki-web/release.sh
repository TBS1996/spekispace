#!/bin/bash
dx build --release
if [ -d "dist" ]; then
    rm -r dist
fi
mv ../target/dx/speki-web/release/web/public dist
