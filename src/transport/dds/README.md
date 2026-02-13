# DDS Transport Implementation

## Library Selection: dust_dds

This transport implementation uses the **dust_dds** crate, not rustdds.

### Decision Rationale (February 2026)

#### **Why dust_dds**

| Factor                         | Details |
|:-------------------------------|:--------|
| ✅ **Actively maintained**     | Updated regularly with bug fixes (last release: 2 months ago) |
| ✅ **Commercial backing**      | Developed by S2E Software Systems with professional support available |
| ✅ **Proven interoperability** | Works with RTI DDS, FastDDS, and CycloneDDS |
| ✅ **Complete async API**      | Native Tokio support without blocking calls |
| ✅ **Proper discovery APIs**   | `StatusCondition` with `PublicationMatched` for deterministic endpoint matching |
| ✅ **ROS2 integration**        | Has rmw-dust-dds middleware for robotics applications |
| ✅ **No unsafe code**          | Pure Rust with stable compiler |
| ✅ **Active development**      | Regular releases fixing discovery and interoperability issues |


#### **Why NOT rustdds**

| Factor                        | Details |
|:------------------------------|:--------|
| ❌ **Not maintained**         | Test failures on master branch, last update 5+ months ago |
| ❌ **Broken communication**   | GitHub issue #399: "Publisher sends data but Subscriber cannot receive" |
| ❌ **26 open issues**         | Many fundamental problems including discovery failures |
| ❌ **No commercial support**  | Individual developer project |
| ❌ **Discovery doesn't work** | DataWriters and DataReaders fail to match even on same machine |
| ❌ **Unimplemented APIs**     | `get_matched_subscriptions()` marked "Unimplemented. Do not use." |

### Implementation History

**Initial attempt (February 11, 2026):**
- Implemented full DDS transport using rustdds (660 lines)
- Used actor model, proper async patterns, comprehensive error handling
- Architecture was sound, but rustdds library fundamentally broken
- Manual integration tests showed zero communication between participants
- Discovery protocol failed to match DataWriters with DataReaders

**Current implementation:** (~1000 lines)
- Switching to dust_dds for v0.7.0 release
- Architecture and patterns from rustdds implementation will be reused
- Expect discovery to work properly with `StatusCondition`-based matching

### Download Statistics (February 2026)

- **rustdds**: 189,730 all-time downloads (but abandoned)
- **dust_dds**: 28,979 all-time downloads, 2,726 recent downloads (growing)

### References

- dust_dds: https://github.com/s2e-systems/dust-dds
- rustdds issues: https://github.com/Atostek/RustDDS/issues
- rustdds #399: Publisher/Subscriber communication failure

---

## Technical Implementation Notes

### Async API Selection

The implementation uses dust_dds's **async API** (`dust_dds::dds_async`) rather than the synchronous API:

- **Module**: `dds_async::{DomainParticipantAsync, DataReaderAsync, DataWriterAsync, ...}`
- **Runtime**: Tokio-native, no blocking calls
- **WaitSet**: `WaitSetAsync` for discovery synchronization
- **Integration**: Seamless with mom-rpc's actor-based concurrency model

The async API allows proper integration with Tokio's executor without blocking worker threads. The synchronous API (`dust_dds::dds_sync`) exists in the same crate but would require `tokio::task::spawn_blocking` for every DDS operation.

### Discovery and Synchronization

**Problem**: DDS discovery is asynchronous. If a client publishes immediately after creating a DataWriter, the server's DataReader may not have discovered it yet, causing the message to be silently lost.

**Solution**: The transport uses `WaitSetAsync` with `StatusCondition` to block publication until discovery completes:

- Before publishing, writers check for matched DataReaders
- If none found, attaches a `StatusCondition` for `PublicationMatched` events
- Waits (with timeout) for at least one DataReader to discover the writer
- Discovery typically completes in 30-50ms on loopback network
- No arbitrary sleeps or polling loops required

This ensures RPC requests are never lost due to discovery timing races.

### Type Name Convention

All DdsEnvelope instances use the constant type name `"DdsEnvelope"` regardless of topic. During development, using topic names as type names prevented communication - changing to a constant type name was one of several fixes required for proper DataWriter/DataReader matching (along with removing the `#[dust_dds(key)]` attribute and implementing proper discovery waiting).

### QoS Configuration

RPC semantics are enforced through QoS policies:
- `Reliability::Reliable` - TCP-like delivery with retries
- `History::KeepLast(1)` - Latest message only (prevents correlation confusion)
- `Durability::Volatile` - No persistence (ephemeral RPC)

### Known Limitations

- **Not stress-tested** - Works for single-client scenarios, but concurrent client behavior and high-burst scenarios are untested
- **History depth** - KeepLast(1) may drop samples under load if readers lag
- **Resource limits** - No tuning of max_samples or max_instances

See [GitHub issue #40] for production hardening roadmap.
