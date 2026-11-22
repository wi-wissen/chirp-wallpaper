# Konzept und Entscheidungen

Kurzfassung der Abstimmung vor dem Start. Ausführlich mit Hintergründen: Konzept-Artefakt und Design-Canvas (Logo E2, Mockups Windows 11 und GNOME).

## Ziel

Freie Neuentwicklung (Cleanroom) einer App wie „Dynamic Theme“: täglich das Bing-Bild als Hintergrund und Sperrbildschirm. Ohne Microsoft Store, ohne Installation, ohne Adminrechte für den Normalfall, robust auf Schulrechnern, die bei jedem Start zurückgesetzt werden.

## Entscheidungen

| Thema | Entscheidung |
|---|---|
| Name | Chirp Wallpaper, Programm `chirp` |
| Logo | E2: Fensterplatz mit drei Möwen über Hügeln bei Sonnenaufgang |
| Sprache | Rust: eine EXE ohne Laufzeit, ein Kern für Windows und GNOME |
| Windows-Oberfläche | Slint im Windows-11-Stil, portable EXE, Infobereich-Symbol, Autostart-Schalter |
| GNOME-Oberfläche | GTK4 + libadwaita, Hintergrundbetrieb per XDG-Autostart (kein Tray in GNOME) |
| Lizenz | MIT |
| Sprachen | 21 Sprachdateien wie im Original (nl-BE nutzt nl), Systemsprache, Standard Englisch |
| Auflösung | UHD (4K) mit Rückfall auf 1920×1080 |

## Wenn kein neues Bild geladen werden kann

* Es wird **nichts geändert**. Das aktuelle Bild bleibt; auf zurückgesetzten Rechnern ist das das Basisbild.
* Ein eigenes Ersatzbild ist nicht nötig: Das **Basisbild** (`chirp seal`, wahlweise eigenes Bild wie das Schullogo) übernimmt diese Rolle.
* Der Grund wird im Status gespeichert. Die App zeigt ihn später dezent an, z. B. „Kein neues Bild (keine Verbindung zu www.bing.com). Weiterhin zu sehen: … vom …“.
* Geplante Aufgaben bleiben still (`--quiet`, Exit-Code 2).

## Die Geschichte zum Bild

Bing liefert zu jedem Bild Titel, Ort, Copyright, einen Absatz Beschreibung (in der Sprache der Region) und einen Suchlink. Alles wird gespeichert und angezeigt:

* Startseite der App: Titel, Ort, Beschreibung, Copyright, „Mehr erfahren“ (Link) und Text kopierbar zum Selbstsuchen.
* `chirp status` gibt dasselbe auf der Kommandozeile aus.

## Plattform-Mechanik

* Windows Desktop: `SystemParametersInfoW`, „Füllen“, JPEG-Qualität 100 % (sonst komprimiert Windows 4K-Bilder).
* Windows Sperrbildschirm: pro Benutzer über `LockScreen.SetImageFileAsync`; für alle und vor der Anmeldung über `PersonalizationCSP` (Admin).
* GNOME: `gsettings`; Sperrbildschirm zeigt automatisch denselben Hintergrund; GDM bleibt unverändert.

## Nächste Schritte

1. Auf echtem Windows testen: Sperrbildschirm aus einer EXE ohne Store-Paket, `PersonalizationCSP` auf Education, Einrichtung für alle Benutzer.
2. GNOME als Flatpak (Hintergrund über das Wallpaper-Portal).
3. Hebräisch: Rechts-nach-links-Darstellung in der Windows-App prüfen.
4. Eigenes Metadaten-Archiv per Action auf GitHub Pages (Verlauf für alle Regionen).
5. Demo-Modus mit KI-Bildern für Screenshots.
6. Code-Signierung (SignPath Foundation) gegen SmartScreen-Warnungen.
