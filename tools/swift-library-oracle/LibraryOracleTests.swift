// The Swift side of glance-library's cross-test: Swift MaCrate's own persistence code (CrateKit, through its
// public and @testable API only) writes golden fixtures from synthetic data, and reads back what the Rust port
// wrote. Copied into a scratch copy of MaCrate's source by run.sh; never part of MaCrate itself.
//
//   ORACLE_OUT     write the goldens here (mode a)
//   ORACLE_LEGACY  a v7-era library.sqlite built by run.sh from legacy-v7.sql (mode a upgrades a copy)
//   ORACLE_RUST    a directory written by `cargo run -p glance-library --example write-fixtures` (mode b)
//   ORACLE_GOLDEN  the goldens mode a wrote (mode b compares against them)
//   ORACLE_REPORT  where mode b writes its report and the library it changed
//
// All data is synthetic: paths live under /Volumes/Fixture, ids are repeated hex digits.

import CryptoKit
import Foundation
import GRDB
import GlanceCore
import XCTest

@testable import CrateKit

// MARK: - The data set (the Rust scenario mirrors it step by step)

enum Fx {
    static let root = "/Volumes/Fixture/Library"
    static let mounted: Set<String> = ["/Volumes/Fixture"]
    static func id(_ c: Character, _ size: Int64) -> String { String(repeating: c, count: 64) + ":\(size)" }

    static let a = id("a", 1000), b = id("b", 2000), c = id("c", 3000), d = id("d", 4000)
    static let e = id("e", 5000), f = id("f", 6000), g = id("1", 7000), r = id("2", 8000)
    static let l = id("3", 9000), m = id("5", 11000)
    static let edgeNames = [".hidden", "name.", "a.b.c.mov", "no_ext", "x.tar.gz", "sp ace.m ov"]
    static func k(_ i: Int) -> String { id("4", Int64(10 + i)) }

    static let pA = root + "/blue_intro.mov"
    static let pB = root + "/Sub Dir/\u{DC}n\u{EF}c\u{F6}d\u{E9} caf\u{E9}.mov"          // NFC
    static let pC = root + "/Sub Dir/U\u{308}ni\u{308}co\u{308}de\u{301} cafe\u{301}.mov"   // NFD
    static let pD = root + "/odd name #%&?'\" .mov"
    static let pE1 = root + "/dup/one.mov"
    static let pE2 = "/Volumes/Fixture Two/Backup/one.mov"
    static let pG = root + "/gone-dismissed.mov"
    static let pF = root + "/gone.mov"
    static let pRBefore = root + "/before rename.mov"
    static let pR = root + "/after rename.mov"
    static let pL = "/private/var/fixture/local/RedForm_4k_01.mov"
    static let pM = root + "/movement.MOV"
    static func pK(_ i: Int) -> String { root + "/edge/" + edgeNames[i] }

    static let s1 = "8F2C0D6E-1B7A-4C3D-9E8F-0A1B2C3D4E5F"
    static let s2 = "0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D"
    static let s3 = "FFFFFFFF-0000-4000-8000-000000000000"

    static var everything: ClipFilter {
        ClipFilter(
            ratingMin: 2, tags: ["blue", "intro"], tagsLogic: .or, bpmRange: 100.5...130, energyRange: 3...7,
            mood: "dark", genre: "techno", hasDuplicates: true,
            loopsOnly: MeasuredFlag(), cycleRange: MeasuredValue(2...8.5),
            exactCyclesOnly: MeasuredFlag(includeNotMeasured: false), joinStepMax: MeasuredValue(2.5),
            noHardCuts: MeasuredFlag(), noFlashes: MeasuredFlag(includeNotMeasured: false),
            movesState: MeasuredValue(.moves, includeNotMeasured: false), noText: MeasuredFlag(),
            ground: MeasuredValue(.onBlack), centreHeavy: MeasuredFlag(), measuredBy: .lab)
    }

    static func row(_ id: String, _ path: String, size: Int64 = 1, mtime: Double = 1_700_000_000, rating: Int = 0,
                    bpm: Double? = nil, mood: String? = nil, genre: String? = nil, energy: Int? = nil,
                    brief: String? = nil, duration: Double? = nil, fps: Double? = nil, w: Int? = nil, h: Int? = nil,
                    created: Double = 1_700_000_000, updated: Double = 1_700_000_000, orphan: Bool = false) -> ClipRow {
        ClipRow(clipID: id, currentPath: path, fileSize: size, fileMtime: mtime, rating: rating, bpm: bpm, mood: mood,
                genre: genre, energy: energy, brief: brief, durationSec: duration, frameRate: fps, videoWidth: w,
                videoHeight: h, createdAt: created, updatedAt: updated, isOrphan: orphan)
    }

    static func measurement(_ id: String, key: String, status: String) -> ClipMeasurement {
        ClipMeasurement(clipID: id, labKey: key, labStatus: status, labReason: nil, pass1Score: nil, isLoop: nil,
                        cycleS: nil, cycleRule: nil, approxCycle: nil, joinStep: nil, cuts: nil, flashes: nil,
                        flicker: nil, flowMean: nil, moves: nil, textChecked: 0, textFound: nil, blackShare: nil,
                        centreShare: nil, xfadeScore: nil, measureVersion: nil, source: nil, measuredAt: nil)
    }

    /// The populated library, step by step.
    static func build(_ store: MetadataStore) async throws {
        try await store.upsert(row(a, pA, size: 1000, mtime: 1_731_500_000.25, rating: 5, bpm: 128.5, mood: "dark",
                                   genre: "techno", energy: 7, brief: "Path-like brief a/b \"quoted\"", duration: 12.5,
                                   fps: 29.97, w: 1920, h: 1080, created: 1_731_600_000.123, updated: 1_731_600_500.5),
                               tags: ["blue", "intro", "loop"])
        try await store.upsert(row(b, pB, size: 2000), tags: ["caf\u{E9}"])
        try await store.upsert(row(c, pC, size: 3000), tags: ["cafe\u{301}"])
        try await store.upsert(row(d, pD, size: 4000, rating: 1, energy: 1, brief: "   ", fps: 0, w: 0, h: 0),
                               tags: ["odd"])
        try await store.upsert(row(e, pE1, size: 5000, created: 1_700_000_050, updated: 1_700_000_060), tags: ["dup", "local"])
        try await store.upsert(row(e, pE2, size: 5000, mood: "calm", genre: "ambient"), tags: ["dup"])
        try await store.upsert(row(g, pG, size: 7000, orphan: true), tags: ["gone"])
        _ = try await store.dismissAllOrphans()
        try await store.upsert(row(f, pF, size: 6000, orphan: true), tags: [])
        try await store.upsert(row(r, pRBefore, size: 8000), tags: ["renamed", "zeta"])
        try await store.setPath(at: pRBefore, to: pR)
        try await store.upsert(row(l, pL, size: 9000, rating: 3, bpm: 100, mood: "bright", genre: "house", energy: 3,
                                   duration: 5), tags: [])
        for i in 0..<edgeNames.count {
            try await store.upsert(row(k(i), pK(i), size: Int64(10 + i), created: 1_700_000_100 + Double(i)), tags: [])
        }
        try await store.upsert(row(m, pM, size: 11000, bpm: 140, energy: 10, duration: 60.25), tags: ["and", "rock"])
        try await store.setDuration(clipID: k(0), 3.5)
        try await store.setFrameRate(clipID: k(1), 25)
        try await store.setVideoSize(clipID: k(2), width: 640, height: 360)

        try await store.savePreset(name: "Empty", filter: .empty, sort: nil)
        try await store.savePreset(name: "  Everything  ", filter: everything, sort: .cycleDesc)
        try await store.savePreset(name: "Empty", filter: ClipFilter(ratingMin: 2), sort: .ratingDesc)
        try await store.savePreset(name: "Doomed", filter: .empty, sort: .random)
        try await store.renamePreset(from: "Doomed", to: "Renamed")
        try await store.savePreset(name: "Gone", filter: .empty, sort: nil)
        try await store.deletePreset(name: "Gone")

        try await store.upsertStash(Stash(id: s1, name: "Set A", createdAt: 1_731_600_000.123, updatedAt: 1_731_600_500.5,
                                          clips: [StashEntry(clipID: a, relativePathHint: "blue_intro.mov"),
                                                  StashEntry(clipID: d, relativePathHint: "odd name #%&?'\" .mov"),
                                                  StashEntry(clipID: e, relativePathHint: "dup/one.mov")]))
        try await store.upsertStash(Stash(id: s2, name: "Empty Stash", createdAt: 1_700_000_000, updatedAt: 1_700_000_000, clips: []))
        try await store.upsertStash(Stash(id: s3, name: "Tmp", createdAt: 1, updatedAt: 2,
                                          clips: [StashEntry(clipID: a, relativePathHint: "blue_intro.mov")]))
        try await store.deleteStash(id: s3)
        try await store.renameStash(id: s1, newName: "Set A (renamed)", updatedAt: 1_731_601_000.75)

        try await store.setAlphaType(clipID: a, .straight)
        try await store.setAlphaProbe(clipID: a, "choosable:looksStraight:1234:0.25")
        try await store.setAlphaProbe(clipID: b, "stated:straight:HAP")
        try await store.setAlphaProbe(clipID: c, "none")
        try await store.setAlphaType(clipID: d, .premultiplied)

        let native = ClipMeasurement(
            clipID: a, labKey: "native-a", labStatus: "measured", labReason: "loops cleanly", pass1Score: 0.42,
            isLoop: 1, cycleS: 4, cycleRule: "whole file loops", approxCycle: 0, joinStep: 1.25, cuts: 0, flashes: 0,
            flicker: 0.012, flowMean: 0.5, moves: 1, textChecked: 1, textFound: 0, blackShare: 0.9, centreShare: 0.45,
            xfadeScore: 0.1, measureVersion: MeasurementScanner.measureVersion, source: MeasurementScanner.source,
            measuredAt: "2026-11-14T16:37:09Z")
        let wrote1 = try await store.insertNativeMeasurement(native)
        var again = native
        again.cuts = 9
        let wrote2 = try await store.insertNativeMeasurement(again)
        var skip = measurement(d, key: "native-d", status: MeasurementScanner.unsupportedStatus)
        skip.labReason = "unsupported-native: png"
        skip.measureVersion = MeasurementScanner.measureVersion
        skip.measuredAt = "2026-11-14T16:40:00Z"
        let wrote3 = try await store.insertNativeMeasurement(skip)
        XCTAssertEqual([wrote1, wrote2, wrote3], [true, false, false])
        var labE = measurement(e, key: "lab-e", status: "measured")
        labE.labReason = "why e"; labE.pass1Score = 1.9; labE.isLoop = 1; labE.cycleS = 12; labE.approxCycle = 1
        labE.joinStep = 3; labE.cuts = 1; labE.flashes = 2; labE.blackShare = 0.1; labE.centreShare = 0.2; labE.moves = 0
        labE.measureVersion = "ltx-lab screen (wave 98 rules)"; labE.source = "lab wave 98"; labE.measuredAt = "2026-11-01T10:00:00Z"
        var labL = measurement(l, key: "lab-l", status: "not a loop")
        labL.labReason = "why l"; labL.isLoop = 0
        try await store.writeMeasurementsForTesting([labE, labL])
    }

    static var allPaths: [String] { [pA, pB, pC, pD, pE1, pE2, pG, pF, pR, pL, pM] + (0..<edgeNames.count).map(pK) }
    static var allIDs: [String] { [a, b, c, d, e, f, g, r, l, m] + (0..<edgeNames.count).map(k) }
}

// MARK: - JSON helpers

enum J {
    static func n(_ v: Any?) -> Any { v ?? NSNull() }
    static func write(_ obj: Any, _ url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed]).write(to: url)
    }
    static func read(_ url: URL) throws -> Any {
        try JSONSerialization.jsonObject(with: Data(contentsOf: url), options: [.fragmentsAllowed])
    }
    /// Round trip through JSONSerialization so two values compare as JSON (numbers by value).
    static func normal(_ v: Any) throws -> NSObject {
        try JSONSerialization.jsonObject(with: JSONSerialization.data(withJSONObject: v, options: [.sortedKeys, .fragmentsAllowed]),
                                         options: [.fragmentsAllowed]) as! NSObject
    }
    static func dbValue(_ v: DatabaseValue) -> Any {
        switch v.storage {
        case .null: return NSNull()
        case .int64(let i): return ["integer", String(i)]
        case .double(let d): return ["real", String(format: "%016llx", d.bitPattern)]
        case .string(let s): return ["text", s]
        case .blob(let data): return ["blob", data.map { String(format: "%02x", $0) }.joined()]
        }
    }
    static func rowDict(_ row: Row) -> [String: Any] {
        var out: [String: Any] = [:]
        for (column, value) in row { out[column] = dbValue(value) }
        return out
    }
    static func canon(_ f: ClipFilter) -> [String: Any] {
        func flag(_ x: MeasuredFlag?) -> Any { x.map { ["includeNotMeasured": $0.includeNotMeasured] } ?? NSNull() }
        return [
            "ratingMin": n(f.ratingMin), "tags": f.tags.sorted(by: { Array($0.unicodeScalars.map(\.value)).lexicographicallyPrecedes($1.unicodeScalars.map(\.value)) }),
            "tagsLogic": f.tagsLogic.rawValue,
            "bpmRange": n(f.bpmRange.map { [$0.lowerBound, $0.upperBound] }), "energyRange": n(f.energyRange.map { [$0.lowerBound, $0.upperBound] }),
            "mood": n(f.mood), "genre": n(f.genre), "hasDuplicates": f.hasDuplicates,
            "loopsOnly": flag(f.loopsOnly), "exactCyclesOnly": flag(f.exactCyclesOnly), "noHardCuts": flag(f.noHardCuts),
            "noFlashes": flag(f.noFlashes), "noText": flag(f.noText), "centreHeavy": flag(f.centreHeavy),
            "cycleRange": n(f.cycleRange.map { ["value": [$0.value.lowerBound, $0.value.upperBound], "includeNotMeasured": $0.includeNotMeasured] }),
            "joinStepMax": n(f.joinStepMax.map { ["value": $0.value, "includeNotMeasured": $0.includeNotMeasured] }),
            "movesState": n(f.movesState.map { ["value": $0.value.rawValue, "includeNotMeasured": $0.includeNotMeasured] }),
            "ground": n(f.ground.map { ["value": $0.value.rawValue, "includeNotMeasured": $0.includeNotMeasured] }),
            "measuredBy": n(f.measuredBy?.rawValue),
        ]
    }
    static func clipRow(_ r: ClipRow) -> [String: Any] {
        [
            "clipID": r.clipID, "currentPath": r.currentPath, "fileSize": r.fileSize, "fileMtime": r.fileMtime,
            "rating": r.rating, "bpm": n(r.bpm), "mood": n(r.mood), "genre": n(r.genre), "energy": n(r.energy),
            "brief": n(r.brief), "durationSec": n(r.durationSec), "frameRate": n(r.frameRate),
            "videoWidth": n(r.videoWidth), "videoHeight": n(r.videoHeight), "createdAt": r.createdAt,
            "updatedAt": r.currentPath == Fx.pR ? "volatile" : r.updatedAt as Any, "isOrphan": r.isOrphan,
        ]
    }
    static func measurement(_ m: ClipMeasurement?) -> Any {
        guard let m else { return NSNull() }
        return [
            "clip_id": m.clipID, "lab_key": m.labKey, "lab_status": m.labStatus, "lab_reason": n(m.labReason),
            "pass1_score": n(m.pass1Score), "is_loop": n(m.isLoop), "cycle_s": n(m.cycleS), "cycle_rule": n(m.cycleRule),
            "approx_cycle": n(m.approxCycle), "join_step": n(m.joinStep), "cuts": n(m.cuts), "flashes": n(m.flashes),
            "flicker": n(m.flicker), "flow_mean": n(m.flowMean), "moves": n(m.moves), "text_checked": m.textChecked,
            "text_found": n(m.textFound), "black_share": n(m.blackShare), "centre_share": n(m.centreShare),
            "xfade_score": n(m.xfadeScore), "measure_version": n(m.measureVersion), "source": n(m.source),
            "measured_at": n(m.measuredAt), "provenance": m.provenance,
        ] as [String: Any]
    }
}

// MARK: - Database dump (same shape as the Rust dumper's)

enum Dump {
    static func file(_ url: URL) throws -> [String: Any] {
        var config = Configuration()
        config.readonly = true
        let q = try DatabaseQueue(path: url.path, configuration: config)
        return try q.read { db in
            var out: [String: Any] = [:]
            let master = try Row.fetchAll(db, sql: "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")
            out["sqlite_master"] = master.map { J.rowDict($0) }
            for p in ["journal_mode", "user_version", "application_id", "page_size", "encoding", "auto_vacuum", "page_count", "freelist_count"] {
                out["pragma_" + p] = J.dbValue(try Row.fetchOne(db, sql: "PRAGMA \(p)").map { $0[0] as DatabaseValue } ?? .null)
            }
            var tables: [String: Any] = [:]
            var indexes: [String: Any] = [:]
            for r in master {
                let type: String = r["type"], name: String = r["name"]
                let sql: String = r["sql"] ?? ""
                let q = name.quotedDatabaseIdentifier
                if type == "index" {
                    indexes[name] = try Row.fetchAll(db, sql: "PRAGMA index_xinfo(\(q))").map(J.rowDict)
                    continue
                }
                guard type == "table" else { continue }
                var t: [String: Any] = [:]
                let info = try Row.fetchAll(db, sql: "PRAGMA table_xinfo(\(q))")
                t["table_xinfo"] = info.map(J.rowDict)
                t["index_list"] = try Row.fetchAll(db, sql: "PRAGMA index_list(\(q))").map(J.rowDict)
                t["foreign_key_list"] = try Row.fetchAll(db, sql: "PRAGMA foreign_key_list(\(q))").map(J.rowDict)
                if sql.hasPrefix("CREATE VIRTUAL") { tables[name] = t; continue }
                let pk = info.filter { ($0["pk"] as Int) > 0 }.sorted { ($0["pk"] as Int) < ($1["pk"] as Int) }
                    .map { ($0["name"] as String).quotedDatabaseIdentifier }
                let withoutRowid = sql.contains("WITHOUT ROWID")
                let select = withoutRowid ? "SELECT *" : "SELECT rowid AS \"__rowid\", *"
                let order = pk.isEmpty ? (withoutRowid ? "" : " ORDER BY rowid") : " ORDER BY " + pk.joined(separator: ", ")
                t["rows"] = try Row.fetchAll(db, sql: "\(select) FROM \(q)\(order)").map(J.rowDict)
                tables[name] = t
            }
            out["tables"] = tables
            out["indexes"] = indexes
            return out
        }
    }

    static func checks(_ url: URL) throws -> [String: Any] {
        let q = try DatabaseQueue(path: url.path)
        return try q.read { db in
            var out: [String: Any] = [:]
            out["integrity_check"] = try String.fetchAll(db, sql: "PRAGMA integrity_check")
            out["foreign_key_check"] = try Row.fetchAll(db, sql: "PRAGMA foreign_key_check").count
            out["migrations"] = try String.fetchAll(db, sql: "SELECT identifier FROM grdb_migrations ORDER BY rowid")
            out["clips"] = try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM clips") ?? -1
            out["clip_search"] = try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM clip_search") ?? -1
            return out
        }
    }

    static func ftsIntegrity(_ url: URL) throws -> Bool {
        let q = try DatabaseQueue(path: url.path)
        try q.write { db in try db.execute(sql: "INSERT INTO clip_search(clip_search) VALUES('integrity-check')") }
        return true
    }
}

// MARK: - Reads and queries (identical shape on the Rust side)

enum Reads {
    static func all(_ store: MetadataStore) async throws -> [String: Any] {
        var out: [String: Any] = [:]
        out["fetchAll"] = try await store.fetchAll().map(J.clipRow)
        out["fetchAllUnderRoot"] = try await store.fetchAll(underPathPrefix: Fx.root).map(\.currentPath)
        // Arrays, not dictionaries: Swift's String equality is canonical equivalence, so the NFC and NFD twin paths
        // (and tags) would collapse into one dictionary key.
        var byPath: [Any] = []
        for p in Fx.allPaths + ["/Volumes/Fixture/Library/missing.mov"] {
            byPath.append([p, [
                "tags": try await store.tags(forPath: p),
                "fetchAt": J.n(try await store.fetch(at: p).map(J.clipRow)),
                "fetchByPath": J.n(try await store.fetchByPath(p)?.clipID),
                "alphaType": J.n(try await store.alphaType(forPath: p)?.rawValue),
            ] as [String: Any]])
        }
        out["byPath"] = byPath
        var byID: [String: Any] = [:]
        for id in Fx.allIDs + [Fx.id("9", 1)] {
            let alpha = try await store.alphaInfo(clipID: id)
            byID[id] = [
                "tags": try await store.tags(for: id),
                "currentPaths": try await store.currentPaths(clipID: id),
                "alphaType": J.n(alpha.alphaType?.rawValue), "probe": J.n(alpha.probe),
                "measurement": J.measurement(try await store.measurement(clipID: id)),
                "rowsForClipID": try await store.fetchRows(forClipID: id).map(\.currentPath),
                "fetchClipID": J.n(try await store.fetch(clipID: id)?.currentPath),
            ] as [String: Any]
        }
        out["byID"] = byID
        let allTags = try await store.allTags()
        out["allTags"] = allTags
        var withTag: [Any] = []
        for t in allTags + ["nope"] { withTag.append([t, try await store.clipIDs(withTag: t)]) }
        out["clipIDsWithTag"] = withTag
        out["allMoods"] = try await store.allMoods()
        out["allGenres"] = try await store.allGenres()
        out["clipIDsWithDuplicates"] = try await store.clipIDsWithDuplicates()
        out["countOrphans"] = try await store.countOrphans()
        out["countDismissedOrphans"] = try await store.countDismissedOrphans()
        out["fetchOrphans"] = try await store.fetchOrphans().map(\.currentPath)
        out["fetchDismissedOrphans"] = try await store.fetchDismissedOrphans().map(\.currentPath)
        var totals: [String: Any] = [:]
        for p in [Fx.root, "/Volumes/Fixture", "/Volumes/Fixture Two", "/Volumes/Fix", "/private/var/fixture", Fx.root + "/edge", "/nowhere"] {
            totals[p] = [
                "totalFileSize": try await store.totalFileSize(underPathPrefix: p),
                "hasAnyRow": try await store.hasAnyRow(underPathPrefix: p),
                "unbriefedCount": try await store.unbriefedDistinctCount(underPathPrefix: p),
                "unbriefedEntries": try await store.unbriefedDistinctEntries(underPathPrefix: p).map { [$0.clipID, $0.samplePath] },
            ] as [String: Any]
        }
        out["underPrefix"] = totals
        out["unbriefedCountIn"] = try await store.unbriefedDistinctCount(in: Fx.allPaths)
        out["unbriefedEntriesIn"] = try await store.unbriefedDistinctEntries(in: Fx.allPaths).map { [$0.clipID, $0.samplePath] }
        out["presets"] = try await store.listPresets().map {
            ["name": $0.name, "sort": J.n($0.sort?.rawValue), "createdAt": "volatile", "filter": J.canon($0.filter)] as [String: Any]
        }
        out["stashes"] = try await store.listStashes().map {
            ["id": $0.id, "name": $0.name, "createdAt": $0.createdAt, "updatedAt": $0.updatedAt,
             "clips": $0.clips.map { [$0.clipID, $0.relativePathHint] }] as [String: Any]
        }
        out["unmeasuredClips"] = try await store.unmeasuredClips().map { [$0.path, $0.clipID] }
        out["unmeasuredClipsIn"] = try await store.unmeasuredClips(paths: Fx.allPaths).map { [$0.path, $0.clipID] }
        // The batched path maps are Swift dictionaries keyed by path: ask for the NFC twin and the NFD twin apart.
        let noNFD = Fx.allPaths.filter { $0.unicodeScalars.elementsEqual(Fx.pC.unicodeScalars) == false }
        func cellJSON(_ cells: [String: ClipCellMetadata]) -> [String: Any] { cells.mapValues { c -> [String: Any] in
            ["clipID": J.n(c.clipID), "durationSec": J.n(c.durationSec), "frameRate": J.n(c.frameRate),
             "videoWidth": J.n(c.videoWidth), "videoHeight": J.n(c.videoHeight), "rating": J.n(c.rating),
             "bpm": J.n(c.bpm), "energy": J.n(c.energy), "mood": J.n(c.mood), "genre": J.n(c.genre),
             "brief": J.n(c.brief), "tags": J.n(c.tags)]
        } }
        out["cellMetadata"] = cellJSON(try await store.cellMetadataByPath(noNFD + ["/nowhere.mov"]))
        out["cellMetadataNFD"] = cellJSON(try await store.cellMetadataByPath([Fx.pC]))
        out["tagsByPath"] = try await store.tagsByPath(noNFD)
        out["tagsByPathNFD"] = try await store.tagsByPath([Fx.pC])
        out["tagsByClipID"] = try await store.tagsByClipID(Fx.allIDs)
        out["fetchDuplicates"] = try await store.fetchDuplicates(of: Fx.e, excluding: Fx.pE1).map(\.currentPath)
        out["fetchRowsClipIDs"] = try await store.fetchRows(clipIDs: Fx.allIDs).map(\.currentPath)
        out["fetchRowsPaths"] = try await store.fetchRows(paths: Fx.allPaths).map(\.currentPath)
        return out
    }

    static let searches = ["", "   ", "blue", "blu", "caf\u{E9}", "cafe\u{301}", "&", "()", "rock AND", "RedForm_4k",
                           "mov", "dup one", "#%&?", "a.b", "\u{FC}n\u{EF}", "U\u{308}ni", "dark", "zeta", "renamed",
                           "path quoted", "edge", "hidden", "tar", "ace", "1920", "ON\u{301}E", "\u{301}blue", "x \u{301}y"]

    static var filters: [(String, ClipFilter)] {
        var out: [(String, ClipFilter)] = [
            ("empty", .empty), ("rating1", ClipFilter(ratingMin: 1)),
            ("tagsAnd", ClipFilter(tags: ["blue", "intro"], tagsLogic: .and)),
            ("tagsOr", ClipFilter(tags: ["blue", "dup", "odd"], tagsLogic: .or)),
            ("tagsNfd", ClipFilter(tags: ["cafe\u{301}"])),
            ("bpm", ClipFilter(bpmRange: 100...130)), ("energy", ClipFilter(energyRange: 3...7)),
            ("mood", ClipFilter(mood: "dark")), ("genre", ClipFilter(genre: "house")), ("moodEmpty", ClipFilter(mood: "")),
            ("dups", ClipFilter(hasDuplicates: true)),
            ("loops", ClipFilter(loopsOnly: MeasuredFlag())), ("loopsStrict", ClipFilter(loopsOnly: MeasuredFlag(includeNotMeasured: false))),
            ("cycle", ClipFilter(cycleRange: MeasuredValue(2...8))),
            ("exact", ClipFilter(exactCyclesOnly: MeasuredFlag(includeNotMeasured: false))),
            ("join", ClipFilter(joinStepMax: MeasuredValue(2.5, includeNotMeasured: false))),
            ("cuts", ClipFilter(noHardCuts: MeasuredFlag())), ("flashes", ClipFilter(noFlashes: MeasuredFlag(includeNotMeasured: false))),
            ("moves", ClipFilter(movesState: MeasuredValue(.moves))), ("still", ClipFilter(movesState: MeasuredValue(.still, includeNotMeasured: false))),
            ("noText", ClipFilter(noText: MeasuredFlag())), ("noTextStrict", ClipFilter(noText: MeasuredFlag(includeNotMeasured: false))),
            ("onBlack", ClipFilter(ground: MeasuredValue(.onBlack))), ("fullFrame", ClipFilter(ground: MeasuredValue(.fullFrame, includeNotMeasured: false))),
            ("centre", ClipFilter(centreHeavy: MeasuredFlag())),
            ("byLab", ClipFilter(measuredBy: .lab)), ("byNative", ClipFilter(measuredBy: .native)), ("byNone", ClipFilter(measuredBy: .notMeasured)),
            ("everything", Fx.everything),
        ]
        var combo = ClipFilter(ratingMin: 0, tags: ["dup"])
        combo.loopsOnly = MeasuredFlag()
        out.append(("combo", combo))
        return out
    }

    struct Case { let name: String; let search: String; let filter: ClipFilter; let sort: ClipSort }

    static var cases: [Case] {
        var out: [Case] = []
        for (name, f) in filters { out.append(Case(name: "filter-" + name, search: "", filter: f, sort: .filenameAZ)) }
        for s in ClipSort.allCases { out.append(Case(name: "sort-" + s.rawValue, search: "", filter: .empty, sort: s)) }
        for (i, s) in searches.enumerated() { out.append(Case(name: "search-\(i)", search: s, filter: .empty, sort: .dateAddedDesc)) }
        out.append(Case(name: "combo-blue-rated", search: "blue", filter: ClipFilter(ratingMin: 4), sort: .bpmDesc))
        out.append(Case(name: "combo-dup-loops", search: "one", filter: ClipFilter(hasDuplicates: true, loopsOnly: MeasuredFlag()), sort: .cycleAsc))
        out.append(Case(name: "combo-everything", search: "blue", filter: Fx.everything, sort: .loopScoreAsc))
        return out
    }

    static func queries(_ store: MetadataStore) async throws -> [[String: Any]] {
        let probe = try DatabaseQueue()
        var out: [[String: Any]] = []
        for c in cases {
            let plan = MetadataStore.buildQueryPlan(search: c.search, filter: c.filter, sort: c.sort, mountedVolumes: Fx.mounted)
            let pathPlan = MetadataStore.buildQueryPlanForPaths(search: c.search, filter: c.filter, sort: c.sort, mountedVolumes: Fx.mounted)
            let bare = MetadataStore.buildQueryPlan(search: c.search, filter: c.filter, sort: c.sort, mountedVolumes: nil)
            func args(_ p: MetadataStore.QueryPlan) throws -> [Any] {
                let count = p.sql.filter { $0 == "?" }.count
                guard count > 0 else { return [] }
                let row = try probe.read { db in
                    try Row.fetchOne(db, sql: "SELECT " + Array(repeating: "?", count: count).joined(separator: ", "), arguments: p.arguments)
                }
                return row.map { r in r.databaseValues.map(J.dbValue) } ?? []
            }
            var paths = try await store.queryPaths(search: c.search, filter: c.filter, sort: c.sort)
            var ids = try await store.query(search: c.search, filter: c.filter, sort: c.sort)
            // By scalars, not Swift's `<` (which calls the NFC and NFD twin paths equal).
            let byScalars: (String, String) -> Bool = { $0.unicodeScalars.map(\.value).lexicographicallyPrecedes($1.unicodeScalars.map(\.value)) }
            if c.sort == .random { paths.sort(by: byScalars); ids.sort(by: byScalars) }
            out.append([
                "name": c.name, "search": c.search, "sort": c.sort.rawValue,
                "filterJSON": String(decoding: try JSONEncoder().encode(c.filter), as: UTF8.self),
                "filter": J.canon(c.filter),
                "sql": plan.sql, "args": try args(plan), "pathsSQL": pathPlan.sql, "bareSQL": bare.sql, "bareArgs": try args(bare),
                "paths": paths, "ids": ids, "random": c.sort == .random,
            ])
        }
        return out
    }

    static func searchPaths(_ store: MetadataStore) async throws -> [Any] {
        var out: [Any] = []
        for s in searches {
            let paths = try await store.searchPaths(query: s)
            // Swift MaCrate 0.16.4's `searchClipIDs` traps (Dictionary(uniqueKeysWithValues:)) when the NFC and the
            // NFD twin paths both match: they are one key to a Swift dictionary.
            let twins = paths.contains { $0.unicodeScalars.elementsEqual(Fx.pB.unicodeScalars) }
                && paths.contains { $0.unicodeScalars.elementsEqual(Fx.pC.unicodeScalars) }
            let ids: Any = twins ? "swift-traps" : try await store.searchClipIDs(filenameQuery: s)
            out.append(["search": s, "paths": paths, "ids": ids, "expression": MetadataStore.sanitizeSearchQuery(s)] as [String: Any])
        }
        return out
    }
}

// MARK: - The sidecar / stash / filter byte cases

enum Bytes {
    static var sidecars: [(String, ClipMetadata)] {
        [
            ("nil-everything", ClipMetadata.makeDefault(clipID: Fx.a, now: Date(timeIntervalSince1970: 1_700_000_000))),
            ("full-fractional", ClipMetadata(
                clipID: Fx.b, rating: 5, tags: ["blue", "intro", "\u{FC}n\u{EF}", "\u{65E5}\u{672C}", "a/b", "cafe\u{301}"], bpm: 128.5,
                mood: "dark/\"moody\"", genre: "techno", energy: 7,
                brief: "Path a/b \"quoted\" \\ back\ttab\nnewline \u{1} \u{7F} \u{2028} \u{1F600} caf\u{E9}",
                createdAt: 1_731_600_000.123, updatedAt: 1_731_600_500.5)),
            ("integral", ClipMetadata(clipID: Fx.c, rating: 0, tags: [], bpm: 120, mood: "", genre: nil, energy: 10,
                                      brief: "", createdAt: 1_700_000_000, updatedAt: 1_700_000_001)),
            ("odd-numbers", ClipMetadata(version: 2, clipID: Fx.d, rating: 3, tags: ["x"], bpm: 0.1, energy: 1,
                                         createdAt: 0.00001, updatedAt: 9_007_199_254_740_994)),
            ("big-small", ClipMetadata(clipID: Fx.e, bpm: 1e16, createdAt: -1.5, updatedAt: 123_456_789.000_001)),
        ]
    }

    static let decodeInputs: [String] = [
        #"{"clip_id":"x:1","created_at":1,"updated_at":2}"#,
        #"{"clip_id":"x:1","created_at":1}"#,
        #"{"created_at":1,"updated_at":2}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"rating":null,"tags":null,"version":null,"bpm":null}"#,
        #"{"clip_id":"x:1","created_at":"1","updated_at":2}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"rating":4.0}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"rating":4.5}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"energy":7.0,"bpm":120}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"loop_start_sec":1.5,"unknown":{"a":[1,2]}}"#,
        #"{"clip_id":1,"created_at":1,"updated_at":2}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"tags":["a",1]}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"rating":true}"#,
        #"{"clip_id":"x:1","created_at":1e3,"updated_at":-2.5e-1,"version":7}"#,
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"clip_id":"y:2"}"#,
        #"{}"#, #"[]"#, #"not json"#, "",
        #"{"clip_id":"x:1","created_at":1,"updated_at":2,"rating":9223372036854775808}"#,
        #"{"clip_id":"é😀\/","created_at":1,"updated_at":2}"#,
    ]

    static var stashes: [Stash] {
        [
            Stash(id: Fx.s1, name: "Set A", createdAt: 1_731_600_000.123, updatedAt: 1_731_600_500.5,
                  clips: [StashEntry(clipID: Fx.a, relativePathHint: "blue_intro.mov"),
                          StashEntry(clipID: Fx.d, relativePathHint: "odd name #%&?'\" .mov"),
                          StashEntry(clipID: Fx.e, relativePathHint: "dup/one.mov")]),
            Stash(id: Fx.s2, name: "Empty Stash", createdAt: 1_700_000_000, updatedAt: 1_700_000_000, clips: []),
            Stash(id: Fx.s3, name: "\u{DC}n\u{EF}c\u{F6}d\u{E9} / set \"x\"", createdAt: 0.5, updatedAt: 1e-05,
                  clips: [StashEntry(clipID: Fx.b, relativePathHint: "Sub Dir/\u{DC}n\u{EF}c\u{F6}d\u{E9} caf\u{E9}.mov")]),
        ]
    }

    static let stashDecodeInputs: [String] = [
        #"{"stashes":[]}"#,
        #"{"stashes":[{"id":"A","name":"n","createdAt":1,"updatedAt":2,"clips":[{"clipID":"c","relativePathHint":"h"}]}]}"#,
        #"{"stashes":[{"id":"A","name":"n","createdAt":1,"updatedAt":2}]}"#,
        #"{"stashes":[{"id":"A","name":"n","createdAt":1,"updatedAt":2,"clips":[{"clipID":"c"}]}]}"#,
        #"{"stashes":[{"id":"A","name":"n","createdAt":1,"updatedAt":2,"clips":[],"extra":1}],"more":true}"#,
        #"{}"#, #"[]"#, "garbage",
    ]

    static var filters: [(String, ClipFilter)] {
        [("empty", .empty), ("everything", Fx.everything), ("ranges", ClipFilter(bpmRange: 120...130, energyRange: 5...10)),
         ("flags-false", ClipFilter(loopsOnly: MeasuredFlag(includeNotMeasured: false), cycleRange: MeasuredValue(2.5...8, includeNotMeasured: false))),
         ("unicode", ClipFilter(tags: ["caf\u{E9}", "a/b"], mood: "d\"a/rk"))]
    }

    static let filterDecodeInputs: [String] = [
        #"{"tags":[],"tagsLogic":"and"}"#,
        #"{}"#,
        #"{"tags":[],"tagsLogic":"and","loopsOnly":{},"cycleRange":{"value":[2,8]}}"#,
        #"{"tags":["a","a","b"],"tagsLogic":"or","ratingMin":3}"#,
        #"{"bpmRange":[130,120]}"#,
        #"{"bpmRange":[120]}"#,
        #"{"tagsLogic":"xor"}"#,
        #"{"cycleRange":{"includeNotMeasured":false}}"#,
        #"{"measuredBy":"native","hasDuplicates":false}"#,
        #"{"energyRange":[1.5,3]}"#,
        #"{"ground":{"value":"fullFrame"},"movesState":{"value":"still","includeNotMeasured":false}}"#,
        #"{"ratingMin":null,"mood":null}"#,
        "nope",
    ]

    static let alphaStrings = ["none", "choosable:looksStraight:1234:0.25", "choosable:-:0:0.0", "choosable:cantTell:12:1e-05",
                               "choosable:looksPremultiplied:600:0.001", "choosable:bogus:x:y", "choosable:a:b",
                               "stated:straight:HAP", "stated:premultiplied:ProRes 4444", "stated:straight:by:with:colons",
                               "stated:other:HAP", "stated:straight", "", "None", "none:extra"]

    static var alphaReports: [(String, AlphaTypeReport)] {
        [("none", AlphaTypeReport(kind: .none)),
         ("choosable", AlphaTypeReport(kind: .choosable, look: .looksStraight, samples: 1234, share: 0.25)),
         ("choosable-nil", AlphaTypeReport(kind: .choosable, look: nil, samples: 0, share: 0)),
         ("choosable-small", AlphaTypeReport(kind: .choosable, look: .cantTell, samples: 7, share: 1.0 / 3.0)),
         ("stated", AlphaTypeReport(kind: .stated(.straight, by: "HAP"))),
         ("stated-colon", AlphaTypeReport(kind: .stated(.premultiplied, by: "a:b")))]
    }

    static let doubles: [Double] = [0, -0.0, 1, 0.1, 0.5, 2.5, 1e-05, 0.0001, 0.00009999, 1e15, 1e16, 9007199254740992,
                                    9007199254740994, 1700000000, 1731600000.123, 1731600500.5, 123456789.000001,
                                    0.30000000000000004, 5e-324, 1.7976931348623157e308, 29.97, 59.94005994005994, -1.5,
                                    1234567.0, 100, 4.0, 0.40, 1e100, 1e-100, 12345678901234567890.0]

    static let clipPaths = ["/Volumes/Fixture/Library/loop.mov", "/a/.hidden", "/a/b c/d e.f g", "/a/no_ext",
                            "/a/caf\u{E9}.mov", "/a/x.crate-meta.json", "/a/trailing.mov/"]
    static let sidecarNames = [".loop.mov.crate-meta.json", ".crate-meta.json", "..crate-meta.json", "loop.mov.crate-meta.json",
                               ".a.crate-meta.json", ".\u{301}x.crate-meta.json", ".x.crate-meta.jso", ". .crate-meta.json",
                               ".x.CRATE-META.JSON", "..x.crate-meta.json"]
    static let tokenPaths = ["/a/blue_intro.mov", "/a/.hidden", "/a/name.", "/a/a.b.c.mov", "/a/no_ext", "/a/x.tar.gz",
                             "/a/sp ace.m ov", "/a/movement.MOV", "/a/.x.mov", "/a/..", "/a/caf\u{E9}.mov", "/a/b.c/",
                             "/a/e.\u{301}", "/a/x.mov.", "/a/x. mov", "/a/x.m/ov", "/", "/a/x..mov", "/a/x.123", "/a/x.#1",
                             "/a/x.m\u{E9}v", "/a/x.a b", "/a/.x.", "/a/ .mov"]
    static let trims = ["  a  ", "\t\na\r\n", "\u{A0}a\u{3000}", "\u{2028}a\u{85}", "\u{200B}a\u{FEFF}", "\u{1680}a\u{180E}", " ", "", "a b"]
}

// MARK: - Tests

final class LibraryOracleTests: XCTestCase {
    private var env: [String: String] { ProcessInfo.processInfo.environment }

    /// Mode a: write the goldens.
    func testWriteGoldens() async throws {
        guard let outPath = env["ORACLE_OUT"] else { throw XCTSkip("ORACLE_OUT not set") }
        let out = URL(fileURLWithPath: outPath)
        let fm = FileManager.default

        // empty
        let emptyDir = out.appendingPathComponent("empty")
        try? fm.removeItem(at: emptyDir)
        do { _ = try MetadataStore(location: .file(emptyDir.appendingPathComponent("library.sqlite")), mountedVolumes: { Fx.mounted }) }
        try J.write(try Dump.file(emptyDir.appendingPathComponent("library.sqlite")), emptyDir.appendingPathComponent("dump.json"))
        try J.write(try Dump.file(emptyDir.appendingPathComponent("measurements.sqlite")), emptyDir.appendingPathComponent("measurements-dump.json"))

        // populated
        let popDir = out.appendingPathComponent("populated")
        try? fm.removeItem(at: popDir)
        do {
            let store = try MetadataStore(location: .file(popDir.appendingPathComponent("library.sqlite")), mountedVolumes: { Fx.mounted })
            try await Fx.build(store)
            try J.write(try await Reads.all(store), popDir.appendingPathComponent("reads.json"))
            try J.write(try await Reads.queries(store), popDir.appendingPathComponent("queries.json"))
            try J.write(try await Reads.searchPaths(store), popDir.appendingPathComponent("search.json"))
        }
        try J.write(try Dump.file(popDir.appendingPathComponent("library.sqlite")), popDir.appendingPathComponent("dump.json"))
        try J.write(try Dump.file(popDir.appendingPathComponent("measurements.sqlite")), popDir.appendingPathComponent("measurements-dump.json"))
        try J.write(try Dump.checks(popDir.appendingPathComponent("library.sqlite")), popDir.appendingPathComponent("checks.json"))

        // legacy v7 -> upgraded by Swift
        if let legacy = env["ORACLE_LEGACY"] {
            let legacyDir = out.appendingPathComponent("legacy-v7")
            let upDir = out.appendingPathComponent("legacy-v7-upgraded")
            try? fm.removeItem(at: legacyDir)
            try? fm.removeItem(at: upDir)
            try fm.createDirectory(at: legacyDir, withIntermediateDirectories: true)
            try fm.createDirectory(at: upDir, withIntermediateDirectories: true)
            try fm.copyItem(at: URL(fileURLWithPath: legacy), to: legacyDir.appendingPathComponent("library.sqlite"))
            try J.write(try Dump.file(legacyDir.appendingPathComponent("library.sqlite")), legacyDir.appendingPathComponent("dump.json"))
            let lib = upDir.appendingPathComponent("library.sqlite")
            try fm.copyItem(at: URL(fileURLWithPath: legacy), to: lib)
            UserDefaults.standard.removeObject(forKey: "Crate.v0.6.33.forceFullReindex")
            do { _ = try MetadataStore(location: .file(lib), mountedVolumes: { Fx.mounted }) }
            let flag = UserDefaults.standard.bool(forKey: "Crate.v0.6.33.forceFullReindex")
            UserDefaults.standard.removeObject(forKey: "Crate.v0.6.33.forceFullReindex")
            let original = try Data(contentsOf: URL(fileURLWithPath: legacy))
            func same(_ name: String) -> Bool { (try? Data(contentsOf: upDir.appendingPathComponent(name))) == original }
            try J.write(["forceFullReindex": flag, "preV8BackupIsOriginal": same("library.sqlite.pre-v8.bak"),
                         "preMeasurementsBackupIsOriginal": same("library.sqlite.pre-measurements.bak")],
                        upDir.appendingPathComponent("upgrade.json"))
            try J.write(try Dump.file(lib), upDir.appendingPathComponent("dump.json"))
            try J.write(try Dump.checks(lib), upDir.appendingPathComponent("checks.json"))
        }

        // JSON bytes
        let jsonDir = out.appendingPathComponent("json")
        try? fm.removeItem(at: jsonDir)
        try fm.createDirectory(at: jsonDir, withIntermediateDirectories: true)
        for (name, meta) in Bytes.sidecars {
            try ClipMetadata.sidecarEncoder.encode(meta).write(to: jsonDir.appendingPathComponent("sidecar-\(name).json"))
        }
        var decoded: [Any] = []
        for input in Bytes.decodeInputs {
            if let m = try? ClipMetadata.sidecarDecoder.decode(ClipMetadata.self, from: Data(input.utf8)) {
                decoded.append(["input": input, "ok": true,
                                "reencoded": String(decoding: try ClipMetadata.sidecarEncoder.encode(m), as: UTF8.self)])
            } else {
                decoded.append(["input": input, "ok": false])
            }
        }
        try J.write(decoded, jsonDir.appendingPathComponent("sidecar-decode.json"))
        let stashFull = jsonDir.appendingPathComponent("stash-full")
        let stashEmpty = jsonDir.appendingPathComponent("stash-empty")
        try fm.createDirectory(at: stashFull, withIntermediateDirectories: true)
        try fm.createDirectory(at: stashEmpty, withIntermediateDirectories: true)
        try StashFile.write(Bytes.stashes, toRoot: stashFull)
        try StashFile.write([], toRoot: stashEmpty)
        var stashDecoded: [Any] = []
        let scratch = fm.temporaryDirectory.appendingPathComponent("oracle-stash-\(UUID().uuidString)")
        try fm.createDirectory(at: scratch, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: scratch) }
        for input in Bytes.stashDecodeInputs {
            try Data(input.utf8).write(to: StashFile.url(atRoot: scratch))
            stashDecoded.append(["input": input, "stashes": StashFile.read(atRoot: scratch).map {
                ["id": $0.id, "name": $0.name, "createdAt": $0.createdAt, "updatedAt": $0.updatedAt,
                 "clips": $0.clips.map { [$0.clipID, $0.relativePathHint] }] as [String: Any]
            }])
        }
        try J.write(stashDecoded, jsonDir.appendingPathComponent("stash-decode.json"))
        for (name, f) in Bytes.filters {
            try JSONEncoder().encode(f).write(to: jsonDir.appendingPathComponent("filter-\(name).json"))
        }
        var filterDecoded: [Any] = []
        for input in Bytes.filterDecodeInputs {
            let f = try? JSONDecoder().decode(ClipFilter.self, from: Data(input.utf8))
            filterDecoded.append(["input": input, "filter": J.n(f.map(J.canon))])
        }
        try J.write(filterDecoded, jsonDir.appendingPathComponent("filter-decode.json"))

        // Small rules: names, tokens, sanitiser, trimming, doubles, alpha probe strings, shuffles.
        var rules: [String: Any] = [:]
        rules["sidecarURL"] = Bytes.clipPaths.map { [$0, SidecarStore.sidecarURL(for: URL(fileURLWithPath: $0)).path] }
        rules["clipURLFromSidecar"] = Bytes.sidecarNames.map {
            [$0, J.n(SidecarStore.clipURLFromSidecar(URL(fileURLWithPath: "/r/" + $0))?.path)]
        }
        rules["filenameToken"] = Bytes.tokenPaths.map { [$0, URL(fileURLWithPath: $0).deletingPathExtension().lastPathComponent] }
        rules["sanitize"] = (Reads.searches + ["blue's", "blue_intro", "blue-intro", "\u{3000}x\u{2003}y", "a\u{200D}b", "\u{FF9E}x",
                                               "\u{1F600}smile", "\u{D4E} x", "x \u{D4E} y", "\u{D4E}", "e\u{301}\u{301}", "IV\u{2163}",
                                               "\u{00BD}half", "\u{24B6}circled", "tab\tsep", "\u{0}nul"])
            .map { [$0, MetadataStore.sanitizeSearchQuery($0)] }
        rules["fsRepresentation"] = ["caf\u{E9}.mov", "\u{DC}n\u{EF}c\u{F6}d\u{E9}", "e\u{301}\u{323}x", "\u{1E69}\u{301}",
                                     "\u{AC00}\u{AC01}\u{D7A3}", "\u{1100}\u{1161}", "\u{212B}\u{2126}\u{F900}\u{2F800}", "a\u{0345}\u{0301}\u{05B0}",
                                     "\u{0F73}\u{0F75}\u{0F81}", "\u{1D160}", "\u{FB2A}\u{0958}", "Ç\u{327}", "x\u{0315}\u{0300}\u{05AE}\u{0301}",
                                     "\u{1B06}\u{11099}\u{1133B}", "a\u{316}\u{F73}", "a\u{F73}\u{316}", "\u{1E69}\u{316}",
                                     "a\u{344}\u{316}", "a\u{F75}\u{F71}", "\u{301}\u{316}x"]
            .map { [$0, URL(fileURLWithPath: "/a/" + $0).lastPathComponent] }
        rules["trim"] = Bytes.trims.map { [$0, $0.trimmingCharacters(in: .whitespacesAndNewlines)] }
        rules["doubleDescription"] = Bytes.doubles.map { [String(format: "%016llx", $0.bitPattern), $0.description] }
        rules["alphaEncode"] = Bytes.alphaReports.map { [$0.0, AlphaProbeCodec.encode($0.1)] }
        rules["alphaDecode"] = Bytes.alphaStrings.map { s -> [Any] in
            guard let r = AlphaProbeCodec.decode(s) else { return [s, NSNull()] }
            return [s, AlphaProbeCodec.encode(r)]
        }
        let names = ["Banana.mov", "apple.mov", "Cherry.mov", "\u{E9}clair.mov", "Zebra.mov", "zeta.mov", "10.mov", "9.mov", "_x.mov"]
        func clip(_ name: String, mtime: Double = 0, duration: Double? = nil) -> ClipFile {
            ClipFile(url: URL(fileURLWithPath: "/tmp/fixtures/\(name)"), displayName: name, sizeBytes: 0,
                     mtime: Date(timeIntervalSince1970: mtime), durationSec: duration)
        }
        rules["sortAZ"] = ClipSorting.apply(names.map { clip($0) }, sort: .filenameAZ, randomSeed: 0).map(\.displayName)
        rules["shuffle"] = [0, 1, 999, 0xCAFE, UInt64.max].map { seed -> [Any] in
            [String(seed), ClipSorting.apply((0..<25).map { clip("clip\($0).mov") }, sort: .random, randomSeed: seed).map(\.displayName)]
        }
        try J.write(rules, out.appendingPathComponent("rules.json"))
    }

    /// The Unicode tables the Rust sanitiser needs, generated from this Foundation.
    func testWriteUnicodeTables() throws {
        guard let outPath = env["ORACLE_OUT"] else { throw XCTSkip("ORACLE_OUT not set") }
        func ranges(_ test: (Unicode.Scalar) -> Bool) -> [(UInt32, UInt32)] {
            var out: [(UInt32, UInt32)] = []
            var start: UInt32? = nil
            var last: UInt32 = 0
            for v in UInt32(0)...0x10FFFF {
                guard let s = Unicode.Scalar(v), test(s) else { continue }
                if let st = start, v == last + 1 { last = v; _ = st } else {
                    if let st = start { out.append((st, last)) }
                    start = v; last = v
                }
            }
            if let st = start { out.append((st, last)) }
            return out
        }
        let alnum = ranges { CharacterSet.alphanumerics.contains($0) }
        let space = ranges { CharacterSet.whitespacesAndNewlines.contains($0) }
        let joins = ranges { ("a" + String(Character($0))).count == 1 && (" " + String(Character($0))).count == 1 }
        let prepend = ranges { (String(Character($0)) + " ").count == 1 }
        func table(_ name: String, _ doc: String, _ r: [(UInt32, UInt32)]) -> String {
            var s = "/// \(doc)\npub(crate) static \(name): &[(u32, u32)] = &[\n"
            for chunk in stride(from: 0, to: r.count, by: 6) {
                s += "    " + r[chunk..<min(chunk + 6, r.count)].map { String(format: "(0x%X, 0x%X)", $0.0, $0.1) }.joined(separator: ", ") + ",\n"
            }
            return s + "];\n"
        }
        // What `URL(fileURLWithPath:)` does to each scalar of a path (its file-system representation), Hangul
        // syllables left out (they decompose algorithmically), and the canonical combining classes it reorders by.
        var decomposition: [(UInt32, [UInt32])] = []
        for v in UInt32(0x20)...0x10FFFF where !(0xAC00...0xD7A3).contains(v) && v != 0x2F {
            guard let s = Unicode.Scalar(v) else { continue }
            let mapped = URL(fileURLWithPath: "/x" + String(Character(s))).lastPathComponent.unicodeScalars.dropFirst().map(\.value)
            if mapped != [v] { decomposition.append((v, Array(mapped))) }
        }
        var ccc: [(UInt32, UInt32, UInt8)] = []
        for v in UInt32(0)...0x10FFFF {
            guard let s = Unicode.Scalar(v) else { continue }
            let c = UInt8(s.properties.canonicalCombiningClass.rawValue)
            guard c != 0 else { continue }
            if let lastEntry = ccc.last, lastEntry.1 == v - 1, lastEntry.2 == c { ccc[ccc.count - 1].1 = v } else { ccc.append((v, v, c)) }
        }
        var src = "//! Generated by tools/swift-library-oracle (testWriteUnicodeTables) from Foundation on macOS "
        src += ProcessInfo.processInfo.operatingSystemVersionString + ". Do not edit by hand.\n"
        src += "//! Inclusive scalar ranges, sorted.\n\n"
        src += table("ALPHANUMERICS", "`CharacterSet.alphanumerics` (general categories L*, M*, N*).", alnum) + "\n"
        src += table("WHITESPACES_AND_NEWLINES", "`CharacterSet.whitespacesAndNewlines`.", space) + "\n"
        src += table("JOINS_PREVIOUS", "Scalars that never start a new `Character` after a letter or a space (grapheme Extend, SpacingMark, ZWJ).", joins) + "\n"
        src += table("PREPEND", "Scalars after which a space does not start a new `Character` (grapheme Prepend).", prepend) + "\n"
        src += "/// `URL(fileURLWithPath:)`'s file-system representation of one scalar, where it is not the scalar itself (Hangul\n"
        src += "/// syllables excepted).\npub(crate) static FS_DECOMPOSITION: &[(u32, &[u32])] = &[\n"
        for (v, m) in decomposition {
            src += String(format: "    (0x%X, &[", v) + m.map { String(format: "0x%X", $0) }.joined(separator: ", ") + "]),\n"
        }
        src += "];\n\n/// Canonical combining classes other than 0, as `(first, last, class)`.\npub(crate) static COMBINING_CLASS: &[(u32, u32, u8)] = &[\n"
        for chunk in stride(from: 0, to: ccc.count, by: 4) {
            src += "    " + ccc[chunk..<min(chunk + 4, ccc.count)].map { String(format: "(0x%X, 0x%X, %d)", $0.0, $0.1, $0.2) }.joined(separator: ", ") + ",\n"
        }
        src += "];\n"
        try src.write(to: URL(fileURLWithPath: outPath).appendingPathComponent("unicode_tables.rs"), atomically: true, encoding: .utf8)
    }

    /// Mode b: read back what the Rust port wrote.
    func testReadRustWritten() async throws {
        guard let rustPath = env["ORACLE_RUST"], let goldenPath = env["ORACLE_GOLDEN"], let reportPath = env["ORACLE_REPORT"] else {
            throw XCTSkip("ORACLE_RUST / ORACLE_GOLDEN / ORACLE_REPORT not set")
        }
        let rust = URL(fileURLWithPath: rustPath), golden = URL(fileURLWithPath: goldenPath), report = URL(fileURLWithPath: reportPath)
        let fm = FileManager.default
        try? fm.removeItem(at: report)
        try fm.createDirectory(at: report, withIntermediateDirectories: true)
        var summary: [String: Any] = [:]
        var failures: [String] = []
        func check(_ ok: Bool, _ what: String) { if !ok { failures.append(what) }; XCTAssertTrue(ok, what) }

        // Open each Rust library with MetadataStore; nothing may migrate, the FTS index must already be full.
        for name in ["empty", "populated", "legacy-v7-upgraded"] {
            let src = rust.appendingPathComponent(name)
            guard fm.fileExists(atPath: src.appendingPathComponent("library.sqlite").path) else { failures.append("missing \(name)"); continue }
            let dir = report.appendingPathComponent("opened-" + name)
            try fm.copyItem(at: src, to: dir)
            let lib = dir.appendingPathComponent("library.sqlite")
            let before = try Dump.checks(lib)
            let dumpBefore = try J.normal(try Dump.file(lib))
            do { _ = try MetadataStore(location: .file(lib), mountedVolumes: { Fx.mounted }) }
            let after = try Dump.checks(lib)
            let dumpAfter = try J.normal(try Dump.file(lib))
            check(try J.normal(before) == J.normal(after), "\(name): open changed counts or migrations: \(before) -> \(after)")
            check(dumpBefore == dumpAfter, "\(name): opening with Swift changed the database")
            check((after["migrations"] as? [String])?.count == 12, "\(name): 12 migrations")
            check((after["integrity_check"] as? [String]) == ["ok"], "\(name): integrity_check")
            check((after["foreign_key_check"] as? Int) == 0, "\(name): foreign_key_check")
            check((after["clips"] as? Int) == (after["clip_search"] as? Int), "\(name): FTS rows == clips rows")
            check((try? Dump.ftsIntegrity(lib)) == true, "\(name): FTS integrity-check")
            summary["open-" + name] = after
        }

        // Reads and queries of the Rust-written populated library equal Swift's own.
        let popLib = report.appendingPathComponent("opened-populated/library.sqlite")
        if fm.fileExists(atPath: popLib.path) {
            let store = try MetadataStore(location: .file(popLib), mountedVolumes: { Fx.mounted })
            let reads = try J.normal(try await Reads.all(store))
            let goldenReads = try J.normal(try J.read(golden.appendingPathComponent("populated/reads.json")))
            if let r = reads as? NSDictionary, let g = goldenReads as? NSDictionary {
                var differing: [String] = []
                for key in Set((r.allKeys as! [String]) + (g.allKeys as! [String])) where r[key] as? NSObject != g[key] as? NSObject {
                    differing.append(key)
                }
                check(differing.isEmpty, "reads differ: \(differing.sorted())")
                summary["readsCompared"] = g.count
            }
            let queries = try J.normal(try await Reads.queries(store)) as! NSArray
            let goldenQueries = try J.normal(try J.read(golden.appendingPathComponent("populated/queries.json"))) as! NSArray
            var qDiff: [String] = []
            for (q, gq) in zip(queries, goldenQueries) {
                let qd = q as! NSDictionary, gd = gq as! NSDictionary
                for key in ["paths", "ids", "sql", "args"] where qd[key] as? NSObject != gd[key] as? NSObject {
                    qDiff.append("\(gd["name"] ?? "?").\(key)")
                }
            }
            check(queries.count == goldenQueries.count && qDiff.isEmpty, "queries differ: \(qDiff)")
            summary["queriesCompared"] = goldenQueries.count
            let search = try J.normal(try await Reads.searchPaths(store))
            check(search == (try J.normal(try J.read(golden.appendingPathComponent("populated/search.json")))), "searchPaths differ")
        }

        // Swift writes on the Rust-made library; Rust re-opens it afterwards (run.sh).
        let writeDir = report.appendingPathComponent("after-swift-writes")
        try fm.copyItem(at: rust.appendingPathComponent("populated"), to: writeDir)
        do {
            let lib = writeDir.appendingPathComponent("library.sqlite")
            let store = try MetadataStore(location: .file(lib), mountedVolumes: { Fx.mounted })
            try await store.upsert(Fx.row(Fx.id("6", 12000), Fx.root + "/from swift.mov", size: 12000, rating: 4, mood: "swift",
                                          created: 1_800_000_000, updated: 1_800_000_000.5), tags: ["swifttag", "blue"])
            try await store.setPath(at: Fx.pA, to: Fx.root + "/blue_intro renamed by swift.mov")
            let deleted = try await store.deleteRows(underPathPrefix: Fx.root + "/edge")
            try await store.savePreset(name: "From Swift", filter: ClipFilter(tags: ["swifttag"]), sort: .filenameZA)
            summary["swiftDeleted"] = deleted
            let checks = try Dump.checks(lib)
            check((checks["clips"] as? Int) == (checks["clip_search"] as? Int), "after writes: FTS rows == clips rows")
            summary["afterSwiftWrites"] = checks
        }

        // Rust-written sidecars and stash files decode and re-encode to the same bytes.
        let json = rust.appendingPathComponent("json")
        var sidecarCount = 0
        for name in (try? fm.contentsOfDirectory(atPath: json.path))?.sorted() ?? [] where name.hasPrefix("sidecar-") && !name.hasSuffix("decode.json") {
            let bytes = try Data(contentsOf: json.appendingPathComponent(name))
            let decodedMeta = try ClipMetadata.sidecarDecoder.decode(ClipMetadata.self, from: bytes)
            check(try ClipMetadata.sidecarEncoder.encode(decodedMeta) == bytes, "sidecar \(name) re-encodes to other bytes")
            let goldenBytes = try? Data(contentsOf: golden.appendingPathComponent("json").appendingPathComponent(name))
            check(goldenBytes == bytes, "sidecar \(name) differs from Swift's bytes")
            sidecarCount += 1
        }
        summary["sidecarsCompared"] = sidecarCount
        for (dirName, expected) in [("stash-full", Bytes.stashes), ("stash-empty", [Stash]())] {
            let root = json.appendingPathComponent(dirName)
            let read = StashFile.read(atRoot: root)
            check(read == expected, "\(dirName) decodes to other stashes")
            let tmp = report.appendingPathComponent("rewrite-" + dirName)
            try fm.createDirectory(at: tmp, withIntermediateDirectories: true)
            try StashFile.write(read, toRoot: tmp)
            check((try? Data(contentsOf: StashFile.url(atRoot: tmp))) == (try? Data(contentsOf: StashFile.url(atRoot: root))),
                  "\(dirName) re-encodes to other bytes")
        }

        // The sidecar tree the Rust writer made, scanned by Swift.
        let tree = rust.appendingPathComponent("sidecar-tree")
        if fm.fileExists(atPath: tree.path) {
            let scanned = await SidecarStore.scanSidecars(under: tree)
            let rel = scanned.map { ($0.key.path.replacingOccurrences(of: tree.path + "/", with: ""), $0.value.clipID) }.sorted { $0.0 < $1.0 }
            let direct = await SidecarStore.scanSidecarsDirect(in: tree)
            summary["scan"] = rel.map { [$0.0, $0.1] }
            summary["scanDirect"] = direct.keys.map { $0.lastPathComponent }.sorted()
            let expected = try J.normal(try J.read(tree.appendingPathComponent("../sidecar-tree-scan.json")))
            check(try J.normal(rel.map { [$0.0, $0.1] }) == expected, "sidecar scan differs from the Rust scan")
        }

        summary["failures"] = failures
        try J.write(summary, report.appendingPathComponent("report.json"))
    }
}
