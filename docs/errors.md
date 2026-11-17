# Error codes and messages (ADR 0057)

fs-rpc says WHAT happened (code) and WITH WHAT (typed parameters). It never delivers a finished sentence to a person: the
program owns its MessageFormat 2 files (de, en and more) and words every code itself. `RpcError::from(&error)` (for
`HandshakeError`, `Fault`, `Problem`, `Failure`, `io::Error`) or `RpcError::of(&capnp::Error)` gives the code; the program looks up
`error.message_key()` and fills the placeholders from `error.params()` (text or number). The `Display` of every error type of this
crate is an English fallback for logs; a window must not show it. `RpcError`'s own `Display` is a technical code text
(`rpc.error: no-server (socket=/run/x.sock)`), also for logs only.

This file is a template. The programs own their `.mf2` files; the texts below are recommendations.

## Codes

| Key (`message_key`) | Code | Parameters | When |
|---|---|---|---|
| `rpc-error-no-server` | `no-server` | `socket` (program adds) | nothing listens |
| `rpc-error-socket-unreachable` | `socket-unreachable` | `socket` | another user serves the socket, rights |
| `rpc-error-already-running` | `already-running` | `socket` | a server runs on the socket already |
| `rpc-error-contract-mismatch` | `contract-mismatch` | `running-program`, `running-version`, `running-protocol` (number), `expected-program`, `expected-version`, `expected-protocol` (number); version and program are left out when not known | server is still not the one needed after the replacement |
| `rpc-error-cannot-restart` | `cannot-restart` | `step` (`ask`, `stop`, `terminate`, `not-ended`, `start`, `reach`), `pid` (number, with `terminate`), `kind`, and the running/expected parameters above | the running server could not be replaced |
| `rpc-error-other-program` | `other-program` | `running-program`, `expected-program` | wrong `--socket`: another program answers; left alone |
| `rpc-error-too-old-to-stop` | `too-old-to-stop` | running/expected as above | old server, no stop method, process unknown |
| `rpc-error-timeout` | `timeout` | `server`, `limit-ms` (number) | no answer in time; a write may have happened |
| `rpc-error-connection-lost` | `connection-lost` | `server` | the server went away; offer to connect again |
| `rpc-error-interrupted` | `interrupted` | `server` | a call ended without an answer |
| `rpc-error-unimplemented` | `unimplemented` | `capability` (when the provider named one) | "not possible" (ADR 0045) or an old server |
| `rpc-error-invalid-request` | `invalid-request` | problem extensions (`id`, `kind`, `issue` ...) | the request cannot be accepted |
| `rpc-error-too-large` | `too-large` | `size`, `limit` (numbers, bytes) | does not fit into one message |
| `rpc-error-protocol` | `protocol` | `type` (problem type without base), problem extensions | other problem type, unreadable answer |
| `rpc-error-io` | `io` | `kind` (`invalid-input` ...) | any other system failure |

Codes are never renamed or reused once released (the enum is `non_exhaustive`: a new problem is a new code, so keep a general
fallback message for unknown keys).

## Recommended texts (MessageFormat 2)

```
# en
rpc-error-no-server = No server is running. Start the program again.
rpc-error-contract-mismatch = The running server ({$running-version}) does not fit this window ({$expected-version}). Close the program completely and start it again.
rpc-error-timeout = The server did not answer within {$limit-ms :number} ms.
rpc-error-connection-lost = The connection to the {$server} server was lost.
rpc-error-too-large = This is too large: {$size :number} bytes, at most {$limit :number} fit.
rpc-error-unimplemented = This is not possible here.

# de
rpc-error-no-server = Es laeuft kein Server. Starte das Programm neu.
rpc-error-contract-mismatch = Der laufende Server ({$running-version}) passt nicht zu diesem Fenster ({$expected-version}). Beende das Programm ganz und starte es neu.
rpc-error-timeout = Der Server hat nicht innerhalb von {$limit-ms :number} ms geantwortet.
rpc-error-connection-lost = Die Verbindung zum Server {$server} ist abgebrochen.
rpc-error-too-large = Das ist zu gross: {$size :number} Bytes, hoechstens {$limit :number} passen.
rpc-error-unimplemented = Das ist hier nicht moeglich.
```

Parameters that the table says may be left out (version, program, socket) must not be required by the message, or the message gets a variant without them (MessageFormat 2 `.match`). The keys that are not shown follow the same pattern; each program writes all 15 in all its languages.

## On the wire

Cap'n Proto exceptions carry a kind and a text. fs-rpc packs an RFC 9457 `Problem` into the text (`type`, `title`, `detail`,
`status`, extensions). The `type` is the code, the extensions are the parameters; `title` and `detail` are English sentences
for logs and must not be shown. Proposal for later (append-only, not part of v0.0.3): the schema gets a `code :Text` and typed
`params` next to `title`/`detail`, so the sentences can end.
