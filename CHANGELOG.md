# Changelog

## Version 0.6.1 - 2016-01-12

 * [#90][90]: Edit insert_routes! to avoid type ascription conflicts, and some fixes.

## Version 0.6.0 - 2015-11-24

 * [#81][81]: Remove the extension traits for BodyReader. Closes [#76][76].
 * [#82][82]: Use multirust-rs to install Rust on AppVeyor. Closes [#77][77].
 * [#79][79]: Give the user control over the server's keep-alive policy.
 * [#78][78]: Move hypermedia responsibility from router to handlers.
 * [#75][75]: Remove the logging system in favor of the Log crate. Closes [#71][71].
 * [#74][74]: Implement Handler for Arc. Closes [#70][70].
 * [#73][73]: Use check_path in the send_file[_with_mime] examples. Closes [#72][72].

## Version 0.5.0 - 2015-09-19

 * [#69][69]: Get rid of some of the rough edges regarding MaybeUtf8.
 * [#67][67]: Force connections to close if the thread pool is too congested. Closes [#65][65].
 * [#68][68]: Remove ICE hacks and set the oldest tested Rust version to 1.3. Closes [#17][17].
 * [#66][66]: Automatically gather the features to be tested.
 * [#64][64]: Make wildcards work as path variables.
 * [#63][63]: Restructure the server module.
 * [#62][62]: Move file sending code to Response.
 * [#61][61]: Add an example of a quite minimalistic server.

## Version 0.4.0 - 2015-08-17

 * [#59][59]: Preserve non-UTF-8 data and handle * requests.
 * [#60][60]: Update unicase to 1.0.
 * [#56][56]: Implement a Parameters type. Closes [#55][55].
 * [#58][58]: Test more and forbid warnings and missing docs.

## Version 0.3.1 - 2015-07-30

 * [#50][50]: Implement support for multipart requests. Closes [#49][49].
 * [#52][52]: Bump Rust version for testing on Windows to 1.1.0.

## Version 0.3.0 - 2015-07-10

 * [#48][48]: Revert unicase reexport and add a todo example.
 * [#47][47]: Opt into container based Travis tests.
 * [#45][45]: Fixed size responses. Closes [#37][37].
 * [#39][39]: Update to Hyper 0.6.

## Version 0.2.2 - 2015-07-05

 * [#44][44]: Enhance the insert_routes! macro.
 * [#43][43]: Write more documentation for Context and friends. Closes [#41][41], [#42][42].
 * [#40][40]: Add a Gitter chat badge to README.md.

## Version 0.2.1 - 2015-06-29

 * [#38][38]: Add helper for file loading. Closes [#26][26].

## Version 0.2.0 - 2015-06-23


## Version 0.1.1 - 2015-05-16

[90]: https://github.com/Ogeon/rustful/pull/90
[81]: https://github.com/Ogeon/rustful/pull/81
[82]: https://github.com/Ogeon/rustful/pull/82
[79]: https://github.com/Ogeon/rustful/pull/79
[78]: https://github.com/Ogeon/rustful/pull/78
[75]: https://github.com/Ogeon/rustful/pull/75
[74]: https://github.com/Ogeon/rustful/pull/74
[73]: https://github.com/Ogeon/rustful/pull/73
[69]: https://github.com/Ogeon/rustful/pull/69
[67]: https://github.com/Ogeon/rustful/pull/67
[68]: https://github.com/Ogeon/rustful/pull/68
[66]: https://github.com/Ogeon/rustful/pull/66
[64]: https://github.com/Ogeon/rustful/pull/64
[63]: https://github.com/Ogeon/rustful/pull/63
[62]: https://github.com/Ogeon/rustful/pull/62
[61]: https://github.com/Ogeon/rustful/pull/61
[59]: https://github.com/Ogeon/rustful/pull/59
[60]: https://github.com/Ogeon/rustful/pull/60
[56]: https://github.com/Ogeon/rustful/pull/56
[58]: https://github.com/Ogeon/rustful/pull/58
[50]: https://github.com/Ogeon/rustful/pull/50
[52]: https://github.com/Ogeon/rustful/pull/52
[48]: https://github.com/Ogeon/rustful/pull/48
[47]: https://github.com/Ogeon/rustful/pull/47
[45]: https://github.com/Ogeon/rustful/pull/45
[39]: https://github.com/Ogeon/rustful/pull/39
[44]: https://github.com/Ogeon/rustful/pull/44
[43]: https://github.com/Ogeon/rustful/pull/43
[40]: https://github.com/Ogeon/rustful/pull/40
[38]: https://github.com/Ogeon/rustful/pull/38
[76]: https://github.com/Ogeon/rustful/issues/76
[77]: https://github.com/Ogeon/rustful/issues/77
[71]: https://github.com/Ogeon/rustful/issues/71
[70]: https://github.com/Ogeon/rustful/issues/70
[72]: https://github.com/Ogeon/rustful/issues/72
[65]: https://github.com/Ogeon/rustful/issues/65
[17]: https://github.com/Ogeon/rustful/issues/17
[55]: https://github.com/Ogeon/rustful/issues/55
[49]: https://github.com/Ogeon/rustful/issues/49
[37]: https://github.com/Ogeon/rustful/issues/37
[41]: https://github.com/Ogeon/rustful/issues/41
[42]: https://github.com/Ogeon/rustful/issues/42
[26]: https://github.com/Ogeon/rustful/issues/26
