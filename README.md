# cupcake

Cytube chat recorder for [daiseihai](https://github.com/Hamuko/daiseihai).

cupcake currently only supports Cytube servers that use Engine.IO version 4.
However, all cup-related servers should be already using Engine.IO version 4,
so this should not be an issue.

## Downloads

Pre-built binaries are available on the [releases page](https://github.com/Hamuko/cupcake/releases) for the following platforms:

<table>
    <thead>
        <tr>
            <th>Operating system</th>
            <th colspan=6>Architectures</th>
        </tr>
    </thead>
    <tbody>
        <tr>
            <td>Linux (glibc)</td>
            <td colspan=2 title="aarch64-unknown-linux-gnu">ARM64</td>
            <td colspan=2 title="armv7-unknown-linux-gnueabihf">ARMv7</td>
            <td colspan=2 title="x86_64-unknown-linux-gnu">x86-64</td>
        </tr>
        <tr>
            <td>macOS</td>
            <td colspan=3 title="x86_64-apple-darwin">ARM64</td>
            <td colspan=3 title="aarch64-apple-darwin">x86-64</td>
        </tr>
        <tr>
            <td>Windows</td>
            <td colspan=3 title="aarch64-pc-windows-msvc">ARM64</td>
            <td colspan=3 title="x86_64-pc-windows-msvc">x86-64</td>
        </tr>
    </tbody>
</table>

Latest development builds are also available as build artifacts on the [actions page](https://github.com/Hamuko/cupcake/actions) if you are logged into GitHub.

## Usage

```bash
cupcake [OPTIONS] <DOMAIN> <CHANNEL>
```

For full usage instructions, run `cupcake --help`.

### Filtering messages

Cytube sends all chat messages, including ones from shadow-banned users, to anonymous connections.
This can be prevented by logging in as a guest user using the `--guest-login` option with a unique, non-registered username.
This also means that cupcake is visible in the channel's member list as a guest.
