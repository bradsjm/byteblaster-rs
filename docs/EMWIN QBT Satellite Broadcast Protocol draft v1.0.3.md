# EMWIN QBT Satellite Broadcast Protocol

## Draft Version 1.0.3

---

### WARNING

THIS IS A DRAFT SPECIFICATION DOCUMENT UNDER DEVELOPMENT BY THE US NATIONAL WEATHER SERVICE. IT HAS NOT BEEN VERIFIED FOR ACCURACY OR COMPATIBILITY WITH ANY EXISTING SYSTEMS OR SERVICES. THE USE OF THIS INFORMATION BY ANY PARTY FOR ANY PURPOSE IS AT THEIR OWN RISK. THE GOVERNMENT MAY NOT BE HELD LIABLE FOR ANY DIRECT OR CONSEQUENTIAL DAMAGES ARISING FROM INFORMATION CONTAINED IN THIS DOCUMENT.

---

## Document Control

| Version Number | Date Completed | Author(s) | Version Description |
|----------------|-----------------|-------------|-------------------|
| V 1.0 Draft | 5/12/20?? | Robert Gillespie | Initial Version of Draft Document |
| V 1.0.3 | 12/1/20?? | Robert Gillespie | 5.b.(5) - correct file date/time stamp<br>4.a. & 5 - corrected errors in Header |
| V 1.0.3 | 12/2/20?? | Robert Gillespie | 3.b – added out-of-sequence packet error |

*A portion of the content of this document was provided by Danny Lloyd, Weather Message Software, LLC.*

---

## EMWIN QBT Satellite Broadcast Protocol

### Draft Version 1.0.3

---

## 1. Introduction

a. This document describes the Quick Block Transfer (QBT) protocol used by the U.S. National Weather Service (NWS) for transmission of files over National Environmental Satellite, Data, and Information Service (NESDIS) Geostationary Operational Environmental Satellites (GOES) 13, 14 and 15 [the GOES N/O/P satellite series]. This protocol is unique to the NWS Emergency Managers Weather Information Network (EMWIN).

b. The EMWIN satellite broadcast data stream consists of both text and binary files. Prior to broadcast, each file is divided into a sequence of 1024 byte segments which are encapsulated in 1116 byte QBT packets for transmission. The individual or series of QBT packets for a specific file are numbered sequentially from 1 to N. Upon receipt of packets from the satellite broadcast, receiving system software will reconstitute files from QBT packets.

c. Dividing files into smaller packets allows EMWIN to expedite transmission of higher priority files ahead of lower priority files. This is accomplished by interrupting transmission of lower priority file packets, and allowing higher priority file packets to be transmitted. After higher priority file packets have been transmitted, transmission of lower priority file packets resumes.

d. The EMWIN data stream on GOES-13/14/15 satellites is transmitted at 19,200 kbps, and is not encrypted.

---

## 2. EMWIN Transmission Performance

a. All EMWIN files are assigned a numeric priority. The priority helps determine the order in which files are sent according to the following guidelines:

1. Higher priority (file) packets are transmitted ahead of lower priority (file) packets.
2. The file's packets are transmitted in ascending packet number order, beginning with packet number 1.
3. At any given priority level, packets in a queue will be transmitted in "First In, First Out" (FIFO) order.

b. If receiving system software detects out-of-sequence, missing, incomplete, or malformed packets, the product and its associated sequence of QBT packets may be reported as "bad" or "corrupt" and discarded.

c. The EMWIN satellite broadcast is receive-only, therefore the receiver has no means of notifying the transmitter of any packet loss or errors, nor may it request retransmission of individual packets.

d. Each high priority file is transmitted twice to improve the likelihood of successfully receiving the file. The file retransmission is scheduled to commence no sooner than 5 seconds after the file is first transmitted, but may take longer, depending on the number of existing packets ahead of it in the transmission queue.

---

## 3. QBT Protocol

Each QBT packet is 1116 bytes in length. The QBT packet is composed of the following fields:

### a. Prefix – 6 bytes
**Position 1-6:** 6 bytes of ASCII 0 (null)

### b. Header – 80 bytes

The header consists of the following elements:

#### (1) Product Filename (PF)
**Position 7-21:** Literal "/PF" followed by an 8-character filename, a period, and a three character file extension.

Valid file extensions are:

| Extension | Description |
|-----------|-------------|
| gif | Graphics Interchange Format |
| jpg | Shorter extension for JPEG which stands for Joint Photographic Experts Group |
| png | Portable Network Graphics file format |
| txt | Alphanumeric text format |
| zis | ZIP compressed file format |

#### (2) Packet Number (PN)
**Position 22-30:** Literal "/PN" followed by a left justified number, 1 to 6 bytes in length, identifying the packet's sequence number in the range of 1 to N. Right pad with ASCII 32 (SP) to fill out to byte position 30.

#### (3) Packets Total (PT)
**Position 31-39:** Literal "/PT" followed by a left justified number, 1 to 6 bytes in length, identifying the total number of packets N being sent for this file. Right pad with ASCII 32 (SP) to fill out to byte position 39.

#### (4) Computed Sum (CS)
**Position 40-49:** Literal "/CS" followed by a 7-byte, left justified number identifying the sum of all unsigned byte decimal values in the 1024-byte data block portion of the packet (Section 3.c.). Right pad with ASCII 32 (SP) to fill out to byte position 49.

All bytes in the data block are unsigned (non-negative) values. The individual byte value range is 0 to 255. The resulting maximum value of the computed sum is:

```
1024 bytes × 255 (max value/byte) = 261,120
```

The sum of all byte decimal values is unsigned.

##### Computed Sum Examples

**(a) Text File Example:**

Sending "AcB" in the data block portion of the packet. The computed sum = 230:

| Data Block Byte No. | Text File ASCII Characters | Decimal Unsigned Byte Value | Binary (Hex) | Counted Decimal |
|---------------------|--------------------------|--------------------------|----------------|-----------------|
| 87 | A | 65 | 01000001 | 65 |
| 88 | c | 99 | 01100011 | 99 |
| 89 | B | 66 | 01000010 | 66 |
| 90 | (null) | 0 | 00000000 | 0 |
| : | : | : | : | : |
| 1110 | (null) | 0 | 00000000 | 0 |
| | | | | **COMPUTED SUM: 230** |

**(b) Binary File Example:**

Sending hexadecimal bytes "FF B4 42" in the data portion of the packet. The computed sum = 501:

| Data Block Byte No. | Text File ASCII Characters | Decimal Unsigned Byte Value | Binary File (Hex) | Counted Decimal |
|---------------------|--------------------------|--------------------------|---------------------|-----------------|
| 87 | n/a | 255 | FF | 11111111 | 255 |
| 88 | n/a | 180 | B4 | 10110100 | 180 |
| 89 | B | 66 | 01000010 | 66 |
| 90 | (null) | 0 | 00000000 | 0 |
| : | : | : | : | : |
| 1110 | (null) | 0 | 00000000 | 0 |
| | | | | **COMPUTED SUM: 501** |

#### (5) File Date-Time (FD)
**Position 50-84:** Literal "/FD" followed by the date/time stamp of the file from which the data was received; in left justified format of:

```
MM/DD/YYYY[ASCII 32 (SP)]hh:mm:ss[ASCII 32 (SP)]AM or PM
```

in universal coordinated time (UTC).

**(a) Sequential Fields and Values:**
- Month (MM) = 1-12
- Day (DD) = 1-31
- Year (YYYY) = 20##
- Hour (hh) = 1-12
- Minute (mm) = 00-59
- Seconds (ss) = 00-59

**(b) Single Digit Formatting:**
The field for Month (MM), Day (DD), and hour (hh) will use a single integer digit when the value is less than 10. Leading zeros are not used. For example, January 2, 2016 is formatted as 1/2/2016.

**(c) Padding:**
Append ASCII 32 (SP) to fill the "/FD" field to byte position 84.

#### (6) Separator
**Position 85-86:** ASCII 13 (CR) and ASCII 10 (LF)

### c. Data Block
**Position 87-1110:** 1024-byte block; left justified sequence of bytes from text or binary file.

If the number of bytes from the text or binary file is less than 1024 bytes, an ASCII 0 (null) byte is appended to fill, so that each packet's data block is always 1024 bytes long.

### d. Suffix
**Position 1111-1116:** 6 bytes of ASCII 0 (null)

---

## 4. Example

### a. Header Example
An example of an 80-byte packet header:

```
/PFZFPSFOCA.TXT/PN3    /PT5    /CS63366  /FD5/19/2016 5:24:26 PM        \r\n
```

### b. File Content
The content of NWS weather products placed into 1024-byte data blocks may be alphanumeric text or binary (hexadecimal) representation as prescribed in WMO-No. 386, Manual on the Global Telecommunication System, Annex III to the WMO Technical Regulations, 2015 edition.

The products are not encrypted, but will be compressed if their size exceeds 5kB. Interpretation of the content of products is up to the receiver's software. Compressed products have a file name that ends with .ZIS and use standard ZIP file compression.

---

## 5. QBT Protocol Position Reference Diagram

| Field | Position | Size | Separator | Example Data |
|--------|-----------|--------|------------|---------------|
| Prefix | 1-6 | 6 bytes | x00 |
| /PF | 7-21 | 15 bytes | /PFZFPSFOCA.TXT |
| /PN | 22-30 | 9 bytes | /PN3 |
| /PT | 31-39 | 9 bytes | /PT5 |
| /CS | 40-49 | 10 bytes | /CS63366 |
| /FD | 50-84 | 35 bytes | /FD5/19/2016 5:24:26 PM |
| CR/LF | 85-86 | 2 bytes | x0d x0a |
| Data Block | 87-1110 | 1024 bytes | [1024 data bytes] |
| Suffix | 1111-1116 | 6 bytes | x00 |

---

*End of Document*
