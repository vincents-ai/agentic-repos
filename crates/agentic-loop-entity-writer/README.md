# agentic-loop-entity-writer

Schema-validated entity CRUD for agentic-loop services writing to ADP git refs.

## Features

- **Schema validation**: entities validated before write
- **CRUD operations**: create, read, update, delete
- **Namespace enforcement**: entities written to correct ref namespace

## Usage

```rust
use agentic_loop_entity_writer::{EntityWriter, EntityWriterConfig};

let writer = EntityWriter::new(EntityWriterConfig {
    schema_registry: registry.clone(),
    ref_writer: ref_writer.clone(),
});

writer.create("engram-task", "refs/engram/task/{id}", &task_data).await?;
let data = writer.read("engram-task", "refs/engram/task/{id}").await?;
```

## Tests

9 unit tests.
