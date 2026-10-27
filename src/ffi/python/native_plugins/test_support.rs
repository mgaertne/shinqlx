use alloc::ffi::CString;
use core::ffi::CStr;

use pyo3::{prelude::*, types::PyDict};

pub(super) const REDIS_5: &CStr = c"VERSION = (5, 0, 0)\n";
pub(super) const REDIS_2: &CStr = c"VERSION = (2, 10, 6)\n";

/// A stand-in for the `redis` module that only knows its `VERSION`, restored on drop.
pub(super) struct FakeRedis<'py> {
    modules: Bound<'py, PyDict>,
    previous: Option<Bound<'py, PyAny>>,
}

impl<'py> FakeRedis<'py> {
    pub(super) fn install(py: Python<'py>, code: &CStr) -> Self {
        let modules = py
            .import("sys")
            .and_then(|sys| sys.getattr("modules"))
            .and_then(|modules| Ok(modules.cast_into::<PyDict>()?))
            .expect("could not get sys.modules");
        let previous = modules
            .get_item("redis")
            .expect("could not look up the redis module");
        PyModule::from_code(py, code, c"fake_redis.py", c"redis")
            .expect("could not create the fake redis module");
        Self { modules, previous }
    }
}

impl Drop for FakeRedis<'_> {
    fn drop(&mut self) {
        let _ = match &self.previous {
            Some(previous) => self.modules.set_item("redis", previous),
            None => self.modules.del_item("redis"),
        };
    }
}

const DATABASE_FIXTURES: &str = "
import shinqlx


class Pipeline:
    def __init__(self, database):
        self.database = database
        self.queued = []

    def zadd(self, key, *args):
        members = args[0] if isinstance(args[0], dict) else {args[1]: args[0]}
        self.queued.append(('zadd', key, members))

    def zincrby(self, key, first, second):
        import redis
        value, amount = (first, second) if redis.VERSION < (3,) else (second, first)
        self.queued.append(('zincrby', key, value, amount))

    def hset(self, key, mapping=None):
        if not self.database.hset_supports_mapping:
            raise TypeError('hset() got an unexpected keyword argument')
        self.queued.append(('hset', key, mapping))

    def hmset(self, key, mapping):
        self.queued.append(('hset', key, mapping))

    def incr(self, key):
        self.queued.append(('incr', key))

    def sadd(self, key, *members):
        self.queued.append(('sadd', key, members))

    def lpush(self, key, *values):
        self.queued.append(('lpush', key, values))

    def ltrim(self, key, start, stop):
        self.queued.append(('ltrim', key, start, stop))

    def execute(self):
        for command, key, *args in self.queued:
            if command == 'zadd':
                scores = self.database.zsets.setdefault(key, {})
                for member, score in args[0].items():
                    scores[str(member)] = score
            elif command == 'zincrby':
                self.database.zsets[key][str(args[0])] += args[1]
            elif command == 'incr':
                self.database.strings[key] = str(int(self.database.strings.get(key, 0)) + 1)
            elif command == 'sadd':
                self.database.sets.setdefault(key, set()).update(str(m) for m in args[0])
            elif command == 'lpush':
                self.database.lists.setdefault(key, [])[0:0] = [str(v) for v in reversed(args[0])]
            elif command == 'ltrim':
                self.database.lists[key] = self.database.lists[key][args[0]:args[1] + 1]
            else:
                self.database.hashes[key] = {k: str(v) for k, v in args[0].items()}
        self.queued = []


class Database:
    hset_supports_mapping = True

    def __init__(self, plugin):
        self.permissions = {}
        self.zsets = {}
        self.hashes = {}
        self.strings = {}
        self.lists = {}
        self.sets = {}
        self.flags = {}

    def get_flag(self, player, flag, default=False):
        return self.flags.get((player.steam_id, flag), default)

    def set_flag(self, player, flag, value=True):
        self.flags[(player.steam_id, flag)] = value

    def lrange(self, key, start, stop):
        items = self.lists.get(key, [])
        return items[start:] if stop == -1 else items[start:stop + 1]

    def smembers(self, key):
        return set(self.sets.get(key, set()))

    def zrange(self, key, start, stop, withscores=False):
        items = sorted(self.zsets.get(key, {}).items(), key=lambda item: item[1])
        return items if withscores else [member for member, _ in items]

    def type(self, key):
        for kind, store in (('string', self.strings), ('list', self.lists), ('set', self.sets),
                            ('zset', self.zsets), ('hash', self.hashes)):
            if key in store:
                return kind
        return 'none'

    def has_permission(self, who, level=5):
        return self.permissions.get(who, 0) >= level

    def zcard(self, key):
        return len(self.zsets.get(key, {}))

    def zrangebyscore(self, key, low, high, withscores=False):
        items = [(member, score) for member, score in self.zsets.get(key, {}).items() if score >= low]
        return sorted(items, key=lambda item: item[1])

    def hgetall(self, key):
        return dict(self.hashes.get(key, {}))

    def pipeline(self):
        return Pipeline(self)

    def __contains__(self, key):
        return self.type(key) != 'none'

    def __getitem__(self, key):
        return self.strings[key]

    def __setitem__(self, key, value):
        self.strings[key] = str(value)


class Channel:
    def __init__(self):
        self.replies = []

    def reply(self, message):
        self.replies.append(message)
";

/// Builds a module with a fake `Database`, `Pipeline` and `Channel` and the given further fixtures.
pub(super) fn fixture_module<'py>(py: Python<'py>, fixtures: &CStr) -> Bound<'py, PyModule> {
    let code = CString::new(format!(
        "{DATABASE_FIXTURES}\n{}",
        fixtures.to_str().expect("the fixtures are valid UTF-8")
    ))
    .expect("the fixtures contain no NUL byte");
    PyModule::from_code(py, &code, c"", c"").expect("could not create the fixtures")
}
