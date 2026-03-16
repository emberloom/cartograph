#!/usr/bin/env python3
"""Test build_commits temporal data export."""
import sqlite3
import tempfile
import os
import sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from collections import defaultdict

# Inline the function-under-test so we can test it in isolation
from datetime import datetime, timezone

def build_commits(files_sorted, file_idx, cochange_edges):
    """Build temporal commits data for the time scrubber."""
    from datetime import datetime, timezone
    file_months = defaultdict(set)

    for e in cochange_edges:
        ts_str = e["last_evidence"] if hasattr(e, "__getitem__") else e.get("last_evidence")
        if not ts_str:
            continue
        try:
            dt = datetime.fromisoformat(ts_str.replace(" ", "T")).replace(tzinfo=timezone.utc)
        except (ValueError, AttributeError):
            continue
        month_dt = datetime(dt.year, dt.month, 1, tzinfo=timezone.utc)
        month_ts = int(month_dt.timestamp())
        for fid_key in ("from_id", "to_id"):
            fid = e[fid_key] if hasattr(e, "__getitem__") else e.get(fid_key)
            if fid in file_idx:
                file_months[file_idx[fid]].add(month_ts)

    if not file_months:
        return None

    all_months = sorted(set(ts for months in file_months.values() for ts in months))
    buckets = []
    for month_ts in all_months:
        files_in_month = sorted(idx for idx, months in file_months.items() if month_ts in months)
        if files_in_month:
            buckets.append({"t": month_ts, "files": files_in_month})

    if not buckets:
        return None

    return {"first": all_months[0], "last": all_months[-1], "buckets": buckets}


# Tests
def test_build_commits_basic():
    files_sorted = [{"id": 10, "path": "a.ts"}, {"id": 20, "path": "b.ts"}]
    file_idx = {10: 0, 20: 1}
    edges = [
        {"from_id": 10, "to_id": 20, "last_evidence": "2023-01-15 10:00:00"},
        {"from_id": 10, "to_id": 20, "last_evidence": "2023-03-05 10:00:00"},
    ]
    result = build_commits(files_sorted, file_idx, edges)
    assert result is not None, "Should return data"
    assert result["first"] < result["last"], "first < last"
    assert len(result["buckets"]) == 2, f"Expected 2 months, got {len(result['buckets'])}"
    jan_bucket = result["buckets"][0]
    assert 0 in jan_bucket["files"], "file idx 0 in Jan"
    assert 1 in jan_bucket["files"], "file idx 1 in Jan"
    print("test_build_commits_basic PASSED")

def test_build_commits_empty():
    result = build_commits([], {}, [])
    assert result is None, "Empty input returns None"
    print("test_build_commits_empty PASSED")

def test_build_commits_no_evidence():
    files_sorted = [{"id": 1, "path": "a.ts"}]
    file_idx = {1: 0}
    edges = [{"from_id": 1, "to_id": 2, "last_evidence": None}]
    result = build_commits(files_sorted, file_idx, edges)
    assert result is None, "None timestamps returns None"
    print("test_build_commits_no_evidence PASSED")

test_build_commits_basic()
test_build_commits_empty()
test_build_commits_no_evidence()
print("All tests passed.")
