*ircc is a work in progess
and will gain new features as vinezombie gets them.
Its command-line options WILL change.
Bugs may exist.*

# ircc

[![Chat on libera.chat](https://img.shields.io/badge/libera.chat-%23vinezombie-blueviolet)](https://web.libera.chat/gamja/?channel=#vinezombie)

`ircc` is a low-level IRC client for use in scripts,
comparable to [`ircdog`](https://github.com/ergochat/ircdog).
It uses [`vinezombie`](https://github.com/vinezombie/vinezombie)
to provide direct access to the raw IRC protocol while
automating away some of the tedium.

## Features

- `rustyline`-flavored line editing and history.
- Rate limiting and automatic ping replies.
- TLS support, including client certificates.
- Automatic connection registration, including SASL PLAIN and EXTERNAL.

## Basic Usage

```sh
ircc [-R path/to/register.yml] [--tls] <server>
```

For detailed usage, run `ircc --help`.

For an example `register.yml`, see `example.register.yml`.

## License

`ircc` is licensed under the GNU GPL v3 (only).
Disclosing the source code of bots written using `ircc` to
end users over IRC is also strongly encouraged, but not required.
