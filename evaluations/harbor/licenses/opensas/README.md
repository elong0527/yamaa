# opensas runtime notices

The evaluation image installs the unmodified official opensas v0.6.6 Linux
release executable, independently of yamaa's original MIT-licensed programs.
The installer selects `sas-v0.6.6-amd` or `sas-v0.6.6-arm` from
https://github.com/kirha-ai/opensas/releases/tag/v0.6.6.
The reviewed source ref is `1e8bc655b6a56c58ab20e263e7a2638063834768`.
The release binary is installed as `opensas`; the upstream asset names remain unchanged.
The image retains this directory at `/usr/local/share/licenses/opensas`.

- `LICENSE-APACHE-2.0.txt` is the upstream LICENSE at that source ref.
- `LICENSE-READSTAT-MIT.txt` preserves ReadStat's copyright and permission
  notice, retrieved from WizardMac/ReadStat ref
  `104ba03a8da116eb8c094abc18bc2530b733eda9`. opensas's `src/sas7bdat.zig`
  and `src/sas7bcat.zig` identify portions ported from ReadStat, MIT License,
  Copyright (c) 2013-2016 Evan Miller.
- `LICENSE-ZIG-MIT.txt` preserves the Zig standard library's permission
  notice from ziglang/zig ref `738d2be9d6b6ef3ff3559130c05159ef53336224`.
  opensas uses the Zig standard library. The release does not identify the
  exact compiler revision; this retained notice does not claim it does.

No upstream NOTICE file was found in the reviewed opensas checkout. The
runtime is neither relicensed as yamaa nor modified here. Keep these files
with any redistributed evaluation image. License texts are retained in full;
adding notices does not certify upstream provenance or grant trademark rights.
Remaining provenance and contract questions are tracked in
[issue #1875](https://github.com/elong0527/yamaa/issues/1875).

SAS and all other SAS Institute Inc. product or service names are registered
trademarks or trademarks of SAS Institute Inc. in the USA and other countries.
This work is not affiliated with or endorsed by SAS Institute Inc.
