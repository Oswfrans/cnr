Worker inspected the task and produced a durable claim block.

```cnr-claims
{
  "claims": [
    {
      "subject": "task:example",
      "predicate": "ready_for_review",
      "value": true,
      "confidence": 0.82,
      "evidence": []
    },
    {
      "subject": "task:example",
      "predicate": "tests_run",
      "value": {
        "command": "cargo test",
        "passed": true
      },
      "confidence": 0.9,
      "evidence": []
    }
  ],
  "artifacts": [
    {
      "kind": "diff",
      "path": "artifacts/run-example.diff"
    }
  ]
}
```

