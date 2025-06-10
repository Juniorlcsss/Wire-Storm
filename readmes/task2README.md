# Operation WIRE STORM - RELOADED

## Updated Mission Parameters

The parameters have changed - with one final extension to the protocol.

This final challenge is optional and you are welcome to submit your initial solution. However, if you decide to adapt your solution, we would recommend committing your initial code so the changes are visible.

### CoreTech Message Protocol

Some messages are sensitive and should be validated prior to forwarding to the clients, to ensure data has not become corrupted.

To accommodate for this, the protocol has been extended:

```
    0               1               2               3
    0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7
    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    | MAGIC 0xCC    | OPTIONS       | LENGTH                      |
    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    | CHECKSUM                      | PADDING                     |
    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    | DATA ...................................................... |
    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7

      OPTIONS:

      Bits   0:  Reserved for Future Use.
      Bit    1:  0 = Normal,        1 = Sensitive.
      Bit  2-7:  Padding.

         0     1     2     3     4     5     6     7
      +-----+-----+-----+-----+-----+-----+-----+-----+
      |     |     |                                   |
      | RES | SEN |              PADDING              |
      |     |     |                                   |
      +-----+-----+-----+-----+-----+-----+-----+-----+
```

Two further fields have been added compared to the first stage:
1. CHECKSUM: 16 bits - unsigned - network byte order.
2. OPTIONS: 8 bits - bit 1 indicates a sensitive message.

The checksum field is the 16 bit one's complement of the one's complement sum of all 16 bit words in the header and the data. For purposes of computing the checksum, the value of the checksum field is filled with `0xCC` bytes.

For messages where the the sensitive options bit is `1`, the checksum must be calculated and validated. Any message with an invalid checksum must be dropped with an error logged.

### Further Information

As with the first stage, we will be looking for solutions that are:
* readable
* documented
* efficient

## Tests

To allow for local testing of solutions, our operatives have deployed some Python 3.12 tests for validation.

These can be run without any additional libraries:

```bash
python3 tests.py
```

The tests will output a success message if all pass, or an error message otherwise.

Good luck!
