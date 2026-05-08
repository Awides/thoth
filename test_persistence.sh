#!/bin/bash
set -e

echo "=== Memvid Persistence Test ==="

# Clean start
rm -f thoth.mv2 thoth.mv2.idx
echo "1. Cleaned old memvid files"

# Start app in background
export LD_LIBRARY_PATH=/home/awides/dev/bn/thoth/lib:$LD_LIBRARY_PATH
echo "2. Starting thoth (first run)..."
timeout 30 cargo run --quiet 2>&1 &
THOTH_PID=$!
sleep 5

# Check if files created
if [ -f thoth.mv2 ]; then
    echo "✓ thoth.mv2 created"
    ls -la thoth.mv2
else
    echo "✗ thoth.mv2 NOT created"
fi

# Kill app
kill $THOTH_PID 2>/dev/null || true
wait $THOTH_PID 2>/dev/null || true
sleep 2

echo ""
echo "3. First run complete. File sizes:"
ls -la thoth.mv2 thoth.mv2.idx 2>/dev/null || echo "No files yet"

echo ""
echo "4. Starting second run (should reload messages)..."
timeout 30 cargo run --quiet 2>&1 &
THOTH_PID2=$!
sleep 5

# Kill second instance
kill $THOTH_PID2 2>/dev/null || true
wait $THOTH_PID2 2>/dev/null || true

echo ""
echo "=== Test Complete ==="
echo "Check console output for 'Memvid: loaded' and 'Restoring' messages"
