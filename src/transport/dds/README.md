# DDS Transport Implementation

## Library Selection: dust_dds

This transport implementation uses the **dust_dds** crate, not rustdds.

### Decision Rationale (February 2026)

#### **Why dust_dds**

|                                |  |
|--------------------------------|--|
| ✅ **Actively maintained**     | Updated regularly with bug fixes (last release: 2 months ago) |
| ✅ **Commercial backing**      | Developed by S2E Software Systems with professional support available |
| ✅ **Proven interoperability** | Works with RTI DDS, FastDDS, and CycloneDDS |
| ✅ **Complete async API**      | Native Tokio support without blocking calls |
| ✅ **Proper discovery APIs**   | `StatusCondition` with `PublicationMatched` for deterministic endpoint matching |
| ✅ **ROS2 integration**        | Has rmw-dust-dds middleware for robotics applications |
| ✅ **No unsafe code**          | Pure Rust with stable compiler |
| ✅ **Active development**      | Regular releases fixing discovery and interoperability issues |


#### **Why NOT rustdds**

|                               |  |
|-------------------------------|--|
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

**Current implementation:**
- Switching to dust_dds for v0.7.0 release
- Architecture and patterns from rustdds implementation will be reused
- Expect discovery to work properly with StatusCondition-based matching

### Download Statistics (February 2026)

- **rustdds**: 189,730 all-time downloads (but abandoned)
- **dust_dds**: 28,979 all-time downloads, 2,726 recent downloads (growing)

### References

- dust_dds: https://github.com/s2e-systems/dust-dds
- rustdds issues: https://github.com/Atostek/RustDDS/issues
- rustdds #399: Publisher/Subscriber communication failure
