#!/bin/bash
# Sync project state for AI assistant context
set -euo pipefail

OUTPUT="mom-rpc-sync.tar.gz"

echo "==> Creating AI context archive..."

# Include all tracked files plus staged/unstaged changes
git archive --format=tar HEAD | gzip > "$OUTPUT"

echo "✅ Created: $OUTPUT"
echo "   Upload this to your AI chat to sync project state"
