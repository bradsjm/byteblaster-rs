1. Filenames are not unique and should use a generated id
2. Add support for CAPXML parsing into UGC/VTEC etc.
3. Fix existing parsing issues identified
4. Rework /files endpoint
5. Create UI with map and warnings etc.
6. Create home assistant integration
7. Add vector generation from incidents
8. Add chat bot interface for querying warnings
9. Add alerting system for new warnings


The CAP parser does this work:

Parses XML and validates against CAP schema at j.cs and j.cs.
  Reads root fields:
  identifier
  sent
  status
  msgType
  scope at j.cs.
  Reads first info block fields:
  category
  event
  responseType
  urgency
  severity
  certainty
  expires
  senderName
  headline
  description
  instruction
  web at j.cs.
  Reads eventCode/parameter values:
  SAME
  NationalWeatherService
  PIL
  UGC
  FIPS6
  VTEC
  TIME...MOT...LOC at j.cs, j.cs, and j.cs.
  Reads first polygon-bearing area, first areaDesc, and all resource entries at j.cs.

What Gets Materialized After extraction, the parser maps CAP into the same structures used by raw text products:

  j.o becomes the product id.
  If PIL exists, j.o = text16.Substring(3).
  Else j.o = SAME + "CAP". This is at j.cs.
  j.p becomes the CAP event at j.cs.
  j.m becomes sent at j.cs.
  ad2.c becomes expires, with fallback to sent if missing, at j.cs and j.cs.
  ad2.a becomes county/zone list.
  Preferred source is CAP UGC.
  Fallback source is FIPS6, converted through global::w.m to state/county codes at j.cs.
  ad2.g becomes parsed PVTEC entries from the VTEC parameter at j.cs.
  ad2.i becomes polygon geometry from CAP polygon coordinates at j.cs.
  If category == Met, it also synthesizes a text warning product into j.h/j.i at j.cs. That is display/output formatting only. The raw XML remains in j.g.
