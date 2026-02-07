#!/bin/bash

tar cfvz mom-rpc-sync.tar.gz \
    .gitignore \
    .github \
    CONTRIBUTING.md \
    CHANGELOG.md \
    Cargo.lock \
    Cargo.toml \
    LICENSE \
    README.md \
    docs examples scripts src tests
    
