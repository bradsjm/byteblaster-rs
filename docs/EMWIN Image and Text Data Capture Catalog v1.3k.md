# U.S. NATIONAL WEATHER SERVICE
# EMWIN Image and Text Data Capture Catalog

**Version:** 1.3k

---

## Document Control

| Version | Date Completed | Author(s) | Description |
|---------|---------------|-----------|-------------|
| V 0.1 | 09/14/2016 | Gillespie | Initial Document Draft |
| V 1.0 | 03/01/2017 | Gillespie | Adds title page and Document Control page. Modifies the assigned product header and priority of the Atlantic and Eastern Pacific Basin Tropical Storm Products from NHC. |
| V 1.1 | 10/02/2017 | Banks | Adds storm graphics "Atlantic Basin Tropical Storm [NHC] 3-Day Forecast Track and Current Winds" and "Eastern Pacific Basin Tropical Storm [NHC] 3-Day Forecast Track and Current Winds" |
| V 1.2 | 01/26/2018 | Banks | Modified the URLs and filenames for Titles 22, 23, 24 and 29. Added text for the radar image scaling. |
| V 1.2a | 02/06/2018 | Banks | Corrected URLs for the National Hurricane Center and for the image G02CIRUS.JPG. |
| V 1.2b | 02/22/2018 | Banks | Corrected file name extensions for GOES-R products to match the correct file type. |
| V 1.2c | 06/22/2018 | Banks | Updated the URLs for titles 32, 29, 28 and 27. Changed the file extension for title 32 to PNG. |
| V 1.3 | 07/13/2018 | Banks | Updated entire document to be a single word document. Corrected multiple URLs to use https instead of http. |
| V 1.3a | 08/03/2018 | Banks | Corrected the filename for GOES-N on title 22 from G02CIRUS.JPG to G16CIRCUS.JPG. |
| V 1.3b | 08/17/2018 | Gillespie | NHC Trop Storm Track: AL 3-Day cone replaced by 5-Day cone (Title 44); added EP 5-Day cone product (Title 45). |
| V 1.3c | 12/22/2020 | Banks | Updated the URLs for titles 13, 14, 15, 16, and 17. |
| V 1.3d | 03/09/2021 | Banks | Updated URLs for titles 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17 and 18. |
| V 1.3e | 07/02/2021 | Banks | Updated URLs for titles 20, 25 and 30. |
| V 1.3f | 03/08/2022 | Banks | Added a note to Title 18. |
| V 1.3g | 03/29/2022 | Banks | Updated the URLs for titles 1, 19 and 21. |
| V 1.3h | 04/28/2022 | Banks | Updated all occurrences of DST to DSB. |
| V 1.3i | 08/05/2022 | Banks | Updated the URLs for titles 20 and 25. |
| V 1.3j | 09/09/2022 | Banks | Updated URLs for all of the radar images. |
| V 1.3k | 02/09/2023 | Banks | Added titles 46 and 47 to the document. |

---

*This document is controlled by the Dissemination Systems Branch (DSB), Office of Dissemination, US National Weather Service. Any comments or corrections to this release should be addressed to: nws.emwin.support@noaa.gov*

---

## 1. Overview

The Emergency Managers Weather Information Network (EMWIN) File Image and Text Data Capture Catalog lists the text and image files disseminated by EMWIN. This document defines the "EMWIN broadcast product baseline" by the list of products contained herein.

These products are captured by EMWIN from remote web sites and intended for public dissemination to provide users a heightened sense of situational awareness with regard to weather and water conditions and forecasts. The products described in this document are accessible by the general public, provided they have the commercial systems and services necessary to receive and display the product(s). The products are not encrypted.

---

## 2. Catalog Format

The product listing includes the following elements/characteristics:

a. **Title**
b. **Uniform Resource Locator (URL)**, or file source web address¹
c. **GOES-N series satellite EMWIN broadcast filename**
d. **GOES-R series satellite High Rate Information Transmission (HRIT)/EMWIN broadcast filename²
e. **Description of the World Meteorological Organization (WMO) abbreviated header fields**
f. **EMWIN message priority**
g. **Visual representation of a typical image contained in the file**

> **Note 1:** The public also has the option to directly access the products over the Internet by visiting the product URL. There is however, no assurance that the listed URLs will remain open for public access in the future. Public access to these URLs is controlled by the owning organizations.

> **Note 2:** The "EMWIN GOES-R Filename Convention Document" is found here: http://www.nws.noaa.gov/emwin/index.htm#documents

---

## 3. Product Baseline Management

a. The EMWIN Program Office, Dissemination Systems Branch (DSB), Office of Dissemination, US National Weather Service, maintains this product listing and is the authority for making changes to the product set and individual product descriptions.

b. Questions, comments and recommendations regarding this publication and the product set may be forwarded to the following email for review and consideration: nws.emwin.support@noaa.gov

---

## 4. Abbreviations

| Abbreviation | Explanation |
|-------------|-------------|
| DSB | Dissemination Systems Branch |
| EMWIN | Emergency Managers Weather Information Network |
| GOES | Geostationary Operational Environmental Satellite |
| HRIT | High Rate Information Transmission |
| NOAA | National Oceanic and Atmospheric Administration |
| URL | Uniform Resource Locator |
| US | United States of America |
| WMO | World Meteorological Organization |

---

## 5. EMWIN Image and Text Data Capture Products

> **Note:** All radar images are reduced to 3/4 of the original image size prior to broadcast.

---

### Title-01: Weather Satellite Ephemerides - METEOSAT-7

| Field | Value |
|-------|-------|
| **URL** | https://celestrak.org/NORAD/elements/weather.txt |
| **GOES-N fn** | ephtwous.txt |
| **GOES-R fn** | Z_NOXX10KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-EPHTWOUS.TXT |
| **T1** | N - Notices / O - METNO/WIFMA |
| **T2** | 10 - Unlisted |
| **A1A2** | XX - For use when other designators are not appropriate |
| **ii** | 00 - (arbitrary) |
| **Pri** | 3 |
| **Rev** | 2018-08-17 |

**Sample Data:**
```
1 24932U 97049B 16062.50875964  .00000067  00000-0  00000+0 0  9993
2 24932 10.0557 41.6922 0001990 269.9603 249.4074  0.99856343 55132

1 25338U 98030A 16062.54962326  .00000116  00000-0  00000+0 0  9999
2 25338 98.7829 66.4887 0011450 311.6192 318.9777  1.00280848 49879

1 27509U 02040B 16062.14401054  .00000145  00000-0  00000+0 0  9995
2 27509 4.0124 60.1514 0001744 298.8052 231.0580  1.00273503 47938
```

---

### Title-02: US Radar Cloud Tops

| Field | Value |
|-------|-------|
| **URL** | https://aviationweather.gov/data/obs/radar/radar_nav.gif |
| **GOES-N fn** | radallus.gif |
| **GOES-R fn** | Z_QATA00KKCIddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADALLUS.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-03: US Radar Mosaic - Great Lakes Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/CENTGRLAKES_0.gif |
| **GOES-N fn** | radgrtlk.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADGRTLK.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-04: US Radar Mosaic - Northeast Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/NORTHEAST_0.gif |
| **GOES-N fn** | radnthes.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADNTHES.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-05: US Radar Mosaic - Northern Rockies Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/NORTHROCKIES_0.gif |
| **GOES-N fn** | radrcknt.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADRCKNT.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-06: US Radar Mosaic - Pacific Northwest Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/PACNORTHWEST_0.gif |
| **GOES-N fn** | radpacnw.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADPACNW.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-07: US Radar Mosaic - Pacific Southwest Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/PACSOUTHWEST_0.gif |
| **GOES-N fn** | radpacsw.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADPACSW.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-08: US Radar Mosaic - Southeast Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/SOUTHEAST_0.gif |
| **GOES-N fn** | radsthes.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADSTHES.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-09: US Radar Mosaic - Lower Mississippi Valley Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/SOUTHMISSVLY_0.gif |
| **GOES-N fn** | radsmsvy.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADSMSVY.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-10: US Radar Mosaic - Southern Plains Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/SOUTHPLAINS_0.gif |
| **GOES-N fn** | radsthpl.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADSTHPL.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-11: US Radar Mosaic - Southern Rockies Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/SOUTHROCKIES_0.gif |
| **GOES-N fn** | radrckst.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADRCKST.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-12: US Radar Mosaic - Upper Mississippi Valley Sector [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/UPPERMISSVLY_0.gif |
| **GOES-N fn** | radumsvy.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADUMSVY.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-13: US Radar Mosaic - Alaska [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/ALASKA_0.gif |
| **GOES-N fn** | radallak.gif |
| **GOES-R fn** | Z_QABA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADALLAK.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | B - 90°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-14: Hawaii Radar Mosaic [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/HAWAII_0.gif |
| **GOES-N fn** | radallhi.gif |
| **GOES-R fn** | Z_QAFA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADALLHI.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | F - 90°W - 180° tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-15: US Radar Mosaic [NWS]

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/CONUS_0.gif |
| **GOES-N fn** | radrefus.gif |
| **GOES-R fn** | Z_QATA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADREFUS.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

> **NOTE:** Larger US image link is: https://radar.weather.gov/ridge/lite/CONUS-LARGE_0.gif

---

### Title-16: National Weather Service WSR-88D Image from: GUA

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/GUAM_0.gif |
| **GOES-N fn** | radallgu.gif |
| **GOES-R fn** | Z_QAGA00PGUMddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADALLGU.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | G - 180° - 90°E tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-08-17 |

---

### Title-17: National Weather Service WSR-88D Image from: JUA

| Field | Value |
|-------|-------|
| **URL** | https://radar.weather.gov/ridge/standard/TJUA_0.gif |
| **GOES-N fn** | radallpr.gif |
| **GOES-R fn** | Z_QAEA00TJSJddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-RADALLPR.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | A - Radar data |
| **A1** | E - 0° - 90°W tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 3 |
| **Rev** | 2018-07-10 |

---

### Title-18: NWS Day 3-7 U.S. Hazards Outlook

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/threats/final/hazards_d3_7_contours.png |
| **GOES-N fn** | ushzthrt.png |
| **GOES-R fn** | Z_QGTK98KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-USHZTHRT.PNG |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | G - Significant Weather |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | K - 72 hours forecast |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

> **NOTE:** These products are only created Monday through Friday. Please exercise caution using this outlook during the weekend.

---

### Title-19: East Africa / West Indian Ocean IR [METEOSAT]

| Field | Value |
|-------|-------|
| **URL** | https://www.goes.noaa.gov/FULLDISK/GIIR.JPG |
| **GOES-N fn** | indcirus.jpg |
| **GOES-R fn** | Z_EIIO00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-INDCIRUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | IO - Indian Ocean area |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-20: Eastern Pacific Ocean area IR [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://cdn.star.nesdis.noaa.gov/GOES18/ABI/FD/13/678x678.jpg |
| **GOES-N fn** | g10fdius.jpg |
| **GOES-R fn** | Z_EIPZ00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-G10FDIUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | PZ - Eastern Pacific Area |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-21: Western North Pacific Area IR [MTSAT]

| Field | Value |
|-------|-------|
| **URL** | https://www.goes.noaa.gov/dimg/jma/fd/ir4/10.gif |
| **GOES-N fn** | gms008ja.gif |
| **GOES-R fn** | Z_EIPQ00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-GMS008JA.GIF |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | PQ - Western North Pacific area |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

---

### Title-22: United States IR 2km - CONUS East View [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://cdn.star.nesdis.noaa.gov/GOES16/ABI/CONUS/13/1250x750.jpg |
| **GOES-N fn** | g16cirus.jpg |
| **GOES-R fn** | Z_EINA00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-G16CIRUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | NA - North America |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-23: RA-IV Atlantic Hurricane Basin IR 2km [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://cdn.star.nesdis.noaa.gov/GOES16/ABI/SECTOR/taw/13/900x540.jpg |
| **GOES-N fn** | g02hurus.jpg |
| **GOES-R fn** | Z_EINT00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-G02HURUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | NT - North Atlantic area |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-24: Puerto Rico IR 2km [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://cdn.star.nesdis.noaa.gov/GOES16/ABI/SECTOR/pr/13/600x600.jpg |
| **GOES-N fn** | imgsjupr.jpg |
| **GOES-R fn** | Z_EIPU00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGSJUPR.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | PU - Puerto Rico |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-25: US West Coast IR 4km [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://cdn.star.nesdis.noaa.gov/GOES18/ABI/SECTOR/wus/13/1000x1000.jpg |
| **GOES-N fn** | g10cirus.jpg |
| **GOES-R fn** | Z_EIUS00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-G10CIRUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | I - Infrared |
| **A1A2** | US - United States of America |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-26: Caribbean Surface Analysis [NHC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.nhc.noaa.gov/tafb_latest/CAR_latest.gif |
| **GOES-N fn** | CSA001us.gif |
| **GOES-R fn** | Z_QYEA98KNHCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-CSA001US.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | Y - Observational plotted chart |
| **A1** | E - 0°-90°W tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-27: US Convective Outlook Day 1 [SPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.spc.noaa.gov/products/outlook/day1otlk.gif |
| **GOES-N fn** | moddy1us.gif |
| **GOES-R fn** | Z_QETI00KWNSddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MODDY1US.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | I - 24 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-28: US Convective Outlook Day 2 [SPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.spc.noaa.gov/products/outlook/day2otlk.gif |
| **GOES-N fn** | moddy2us.gif |
| **GOES-R fn** | Z_QETQ00KWNSddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MODDY2US.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | Q - 48 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

---

### Title-29: US Watches, Warnings & Advisories

| Field | Value |
|-------|-------|
| **URL** | https://forecast.weather.gov/wwamap/png/US.png |
| **GOES-N fn** | imgwwaus.png |
| **GOES-R fn** | Z_QGTA98KWNSddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGWWAUS.PNG |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | G - Significant weather |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

---

### Title-30: North American Sounder CAPE [NOAA/GOES] - DEACTIVATED

| Field | Value |
|-------|-------|
| **URL** | http://www.ssd.noaa.gov/PS/PCPN/DATA/RT/NA/GCAPE/20.jpg |
| **GOES-N fn** | imgsndus.jpg |
| **GOES-R fn** | Z_EYUS00KWBCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGSNDUS.JPG |
| **T1** | E - Satellite imagery |
| **T2** | Y - User specified: Convective Available Potential Energy (CAPE) |
| **A1A2** | US - United States of America |
| **ii** | 00 - (arbitrary) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-31: WPC Fronts / NDFD Weather Type [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/basicwx/96fndfd.gif |
| **GOES-N fn** | mod96fbw.gif |
| **GOES-R fn** | Z_QGTI98KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MOD96FBW.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | G - Significant weather |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | I - 24 hours forecast |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-32: US Significant River Flood Outlook [NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/nationalfloodoutlook/finalfop_nobounds.png |
| **GOES-N fn** | gphj88us.png |
| **GOES-R fn** | Z_QGTO88KWNSddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-GPHJ88US.PNG |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | G - Significant weather |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | O - 120 hours forecast (5 days) |
| **ii** | 88 - Ground or water properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

---

### Title-33: US 6-Hour QPF 93e [WPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/qpf/93ewbg.gif |
| **GOES-N fn** | mod93sus.gif |
| **GOES-R fn** | Z_QETC00KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MOD93SUS.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | C - 6 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-34: US Day 1 Excessive Rainfall Outlook [WPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/qpf/94ewbg.gif |
| **GOES-N fn** | mod94sus.gif |
| **GOES-R fn** | Z_QETI88KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MOD94SUS.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | I - 24 hours forecast |
| **ii** | 88 - Ground or water properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-35: US 24-HR Day 1 QPF 94q [WPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/qpf/94qwbg.gif |
| **GOES-N fn** | modqp1us.gif |
| **GOES-R fn** | Z_QETI00KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MODQP1US.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | I - 24 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-08-17 |

---

### Title-36: US 24-HR Day 2 QPF 94q [WPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/qpf/98qwbg.gif |
| **GOES-N fn** | modqp2us.gif |
| **GOES-R fn** | Z_QETQ00KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MODQP2US.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | Q - 48 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-37: US 6-HR QPF [WPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/qpf/fill_91ewbg.gif |
| **GOES-N fn** | mod91eus.gif |
| **GOES-R fn** | Z_QETC00KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-MOD91EUS.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | E - Precipitation |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | C - 6 hours forecast |
| **ii** | 00 - Entire atmosphere (e.g. precipitable water) |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-38: North America 00Z Surface Analysis

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/sfc/namfntsfc00wbg.gif |
| **GOES-N fn** | imgfnt00.gif |
| **GOES-R fn** | Z_QPTA98KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGFNT00.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | P - Pressure |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | A - Analysis (00 hour) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-39: North America 06Z Surface Analysis

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/sfc/namfntsfc06wbg.gif |
| **GOES-N fn** | imgfnt06.gif |
| **GOES-R fn** | Z_QPTC98KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGFNT06.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | P - Pressure |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | C - 6 hours forecast |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-40: North America 12Z Surface Analysis

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/sfc/namfntsfc12wbg.gif |
| **GOES-N fn** | imgfnt12.gif |
| **GOES-R fn** | Z_QPTE98KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGFNT12.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | P - Pressure |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | E - 12 hours forecast |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-41: North America 18Z Surface Analysis

| Field | Value |
|-------|-------|
| **URL** | https://www.wpc.ncep.noaa.gov/sfc/namfntsfc18wbg.gif |
| **GOES-N fn** | imgfnt18.gif |
| **GOES-R fn** | Z_QPTG98KWNHddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-IMGFNT18.GIF |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | P - Pressure |
| **A1** | T - 45°W - 180° northern hemisphere |
| **A2** | G - 18 hours forecast |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

---

### Title-42: Atlantic Basin Tropical Storm [NHC]

| Field | Value |
|-------|-------|
| **URL** | https://www.nhc.noaa.gov/storm_graphics/AT03/AL03YYYY_current_wind_sm2.png |
| **GOES-N fn** | alNNYYrs.png (where NN = Storm Seq No; YY = Calendar Year) |
| **GOES-R fn** | Z_PWEA98KNHCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-ALnnyyRS.PNG |
| **T1** | P - Pictorial information (Binary coded) |
| **T2** | W - Wind |
| **A1** | E - 0° - 90°W tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

> **Notes:**
> - Multiple AT storms per calendar year; folders AT01-AT30 pre-staged at NHC.
> - Files larger than 5K are compressed and presented with .zip file extension.
> - The URL listed was for the third storm of 2018. The URL for this image will be dependent on storms that occur during a calendar year. This image will not be available if storms have not been active.

---

### Title-43: Eastern Pacific Basin Tropical Storm [NHC]

| Field | Value |
|-------|-------|
| **URL** | https://www.nhc.noaa.gov/storm_graphics/EP11/EP11YYYY_current_wind_sm2.png |
| **GOES-N fn** | epNNYYws.png (where NN = Storm Seq No; YY = Calendar Year) |
| **GOES-R fn** | Z_PWEA98KNHCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-EPnnyyWS.PNG |
| **T1** | P - Pictorial information (Binary coded) |
| **T2** | W - Wind |
| **A1** | F - 90°W - 180° tropical belt |
| **A2** | A - Analysis (00 hour) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2018-07-10 |

> **Notes:**
> - Multiple EP storms per calendar year; folders EP01-EP30 pre-staged at NHC.
> - Files larger than 5K are compressed and presented with .zip file extension.
> - The URL listed was for the eleventh storm of 2018. The URL for this image will be dependent on storms that occur during a calendar year. This image will not be available if storms have not been active.

---

### Title-44: Atlantic Basin Tropical Storm [NHC] 5-Day Forecast Track and Current Winds

| Field | Value |
|-------|-------|
| **URL** | https://www.nhc.noaa.gov/storm_graphics/AT03/AL03YYYY_5day_cone_with_line_and_wind_sm2.png |
| **GOES-N fn** | alNNYY5d.png (where NN = Storm Seq No; YY = Calendar Year) |
| **GOES-R fn** | Z_PWEA98KNHCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-ALnnyy5D.PNG |
| **T1** | P - Pictorial information (Binary coded) |
| **T2** | W - Wind |
| **A1** | E - 0° - 90°W tropical belt |
| **A2** | W - (not assigned) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 3 |
| **Rev** | 2018-08-17 |

> **Notes:**
> - Multiple AT storms per calendar year; folders AT01-AT30 pre-staged at NHC.
> - Files larger than 5K are compressed and presented with .zip file extension.
> - The URL listed was for the third storm of 2018. The URL for this image will be dependent on storms that have occurred during a calendar year. This image will not be available if storms have not been active.

---

### Title-45: Eastern Pacific Basin Tropical Storm [NHC] 5-Day Forecast Track and Current Winds

| Field | Value |
|-------|-------|
| **URL** | https://www.nhc.noaa.gov/storm_graphics/EP14/EP14YYYY_5day_cone_with_line_and_wind_sm2.png |
| **GOES-N fn** | epNNYY5d.png (where NN = Storm Seq No; YY = Calendar Year) |
| **GOES-R fn** | Z_PWFW98KNHCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-3-EPnnyy5D.PNG |
| **T1** | P - Pictorial information (Binary coded) |
| **T2** | W - Wind |
| **A1** | F - 90°W - 180° tropical belt |
| **A2** | W - (not assigned) |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 3 |
| **Rev** | 2018-08-17 |

> **Notes:**
> - Multiple EP storms per calendar year; folders EP01-EP30 pre-staged at NHC.
> - Files larger than 5K are compressed and presented with .zip file extension.
> - The URL listed was for the fourteenth storm of 2018. The URL for this image will be dependent on storms that have occurred during a calendar year. This image will not be available if storms have not been active.

---

### Title-46: North Pacific Surface Analysis [OPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://ocean.weather.gov/shtml/arctic/UA_LATEST.gif |
| **GOES-N fn** | NPSA01US.gif |
| **GOES-R fn** | Z_QYPN98...KWNMddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-NPSA01US.gif |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | Y - Observational plotted chart |
| **A1.A2** | PN - North Pacific Area |
| **ii** | 98 - Air properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2022-12-05 |

---

### Title-47: North Pacific Sea Ice [OPC/NOAA]

| Field | Value |
|-------|-------|
| **URL** | https://www.weather.gov/images/afc/ice/CT.jpg |
| **GOES-N fn** | NPIC01US.jpg |
| **GOES-R fn** | Z_QIPN88...PAFCddhhmm_C_KWIN_yyyyMMddhhmmss_nnnnnn-4-NPIC01US.jpg |
| **T1** | Q - Pictorial information regional (Binary coded) |
| **T2** | I - Ice flow |
| **A1.A2** | PN - North Pacific Area |
| **ii** | 88 - Water properties for the Earth's surface |
| **Pri** | 4 |
| **Rev** | 2022-12-05 |

---

*End of Document*
