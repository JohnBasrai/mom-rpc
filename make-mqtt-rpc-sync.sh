#!/bin/bash

tar cfvz make-mqtt-rpc-sync.gz \
    .github \
    .gitignore \
    CHANGELOG.md \
    CONTRIBUTING.md \
    Cargo.lock \
    Cargo.toml \
    LICENSE \
    README.md \
    docs examples scripts src tests
    
