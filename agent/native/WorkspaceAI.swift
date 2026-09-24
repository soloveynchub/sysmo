// Availability only. No user content, model requests or execution tools.
import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif
var status = "requires_macos_26"
#if canImport(FoundationModels)
if #available(macOS 26, *) {
    switch SystemLanguageModel.default.availability {
    case .available: status = "available"
    case .unavailable(let reason):
        switch reason {
        case .appleIntelligenceNotEnabled: status = "apple_intelligence_disabled"
        case .deviceNotEligible: status = "device_not_eligible"
        case .modelNotReady: status = "model_not_ready"
        @unknown default: status = "unavailable"
        }
    }
}
#endif
let data = try JSONSerialization.data(withJSONObject: ["status": status])
FileHandle.standardOutput.write(data)
