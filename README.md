xdg-mime-rs
===========

Xdg-mime-rs is a library that parses the [shared-mime-info][shared-mime]
database and allows querying it to determine the MIME type of a file from
its extension or from its contents.

Xdg-mime-rs is a complete re-implementation of the [xdgmime][xdgmime] C
library, with some added functionality that typically resides in higher
level components, like determining the appropriate icon name for a file
from the [icon theme][fdo-icon-theme].

Installation
------------

Add the following to your `Cargo.toml` file:

```toml
[dependencies]
xdg_mime = "^0.1"
```

or install [`cargo-edit`][cargo-edit] and call:

```
cargo add xdg_mime@0.1
```

Examples
--------

See the [documentation][docs].

Copyright and license
---------------------

Copyright 2020  Emmanuele Bassi

This software is distributed under the terms of the [Apache License
version 2.0](./LICENSE.txt).

[shared-mime]: https://freedesktop.org/wiki/Specifications/shared-mime-info-spec/
[xdgmime]: https://gitlab.freedesktop.org/xdg/xdgmime
[fdo-icon-theme]: https://specifications.freedesktop.org/icon-theme-spec/icon-theme-spec-latest.html
[cargo-edit]: https://github.com/killercup/cargo-edit
[docs]: https://docs.rs/xdg_mime/
