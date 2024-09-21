import InputMethodKit

extension KhiinInputController {
    override func recognizedEvents(_ sender: Any!) -> Int {
        let masks: NSEvent.EventTypeMask = [.keyDown]
        return Int(masks.rawValue)
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        let modifiers = event.modifierFlags
        let changeInputMode = modifiers.contains(.option) && event.keyCode.representative == .punctuation("`")
        let shouldIgnoreCurrentEvent: Bool =
            !changeInputMode && (modifiers.contains(.command) || modifiers.contains(.option))
        
        guard let client: IMKTextInput = sender as? IMKTextInput else {
            return false
        }
        currentOrigin = client.position

        log.debug("Current origin: \(String(describing: currentOrigin))")

        let currentClientID = currentClient?.uniqueClientIdentifierString()
        let clientID = client.uniqueClientIdentifierString()
        if clientID != currentClientID {
            currentClient = client
        }
    
        if (changeInputMode) {
            if (self.isManualMode()) {
                _ = self.commitCurrent();
            }
            self.candidateViewModel.changeInputMode()
            self.reset()
            client.clearMarkedText()
            return true
        } else if (shouldIgnoreCurrentEvent) {
            if (self.isManualMode()) {
                _ = self.commitCurrent();
                self.candidateViewModel.reset()
            }
            return false;
        }

        switch event.keyCode.representative {
            case .alphabet(var char):
                if (self.isManualMode()) {
                    if self.currentDisplayText().hasSuffix("-") && char != self.getHyphenKey() && !self.isIllegal() {
                        _ = self.commitCurrent();
                        self.candidateViewModel.reset()
                    }
                    
                    if (modifiers.contains(.shift) || modifiers.contains(.capsLock)) {
                        // shif xor caplocks
                        char = char.uppercased();
                    }
                } else if (self.isClassicMode()) {
                    if (modifiers.contains(.shift) || modifiers.contains(.capsLock)) {
                        // shif xor caplocks
                        char = char.uppercased();
                    }
                }
                self.candidateViewModel.handleChar(char)
                if (self.isCommited()) {
                    client.insert(self.currentDisplayText())
                    self.reset()
                } else {
                    self.resetWindow()
                    client.mark(self.currentDisplayText())
                }
                return true
            case .number(let num):
                if (modifiers.contains(.shift) && num == 1 && self.isClassicMode()) {
                    self.candidateViewModel.handleChar("!")
                    self.resetWindow()
                    client.mark(self.currentDisplayText())
                    return true
                }

                if (modifiers.contains(.shift) || modifiers.contains(.capsLock)) {
                    if (self.isManualMode()) {
                        _ = self.commitCurrent();
                        self.candidateViewModel.reset()
                    } else {
                        self.reset()
                        client.clearMarkedText()
                    }
                    return false;
                }
                self.candidateViewModel.handleChar(String(num))
                if (self.isManualMode()) {
                    if (self.isCommited()) {
                        client.insert(self.currentDisplayText())
                        self.reset()
                    } else {
                        self.resetWindow()
                        client.mark(self.currentDisplayText())
                    }
                } else if (self.isClassicMode()) {
                    self.resetWindow()
                    client.mark(self.currentDisplayText())
                } else {
                    self.resetWindow()
                }
                return true
            case .punctuation(let ch):
                log.debug("handle punctuation " + ch)
                if (self.isClassicMode()) {
                    if (".,".contains(ch) && !modifiers.contains(.shift)) {
                        self.candidateViewModel.handleChar(ch)
                        self.resetWindow()
                        client.mark(self.currentDisplayText())
                        return true
                    } else if (ch == "/" && modifiers.contains(.shift)) {
                        self.candidateViewModel.handleChar("?")
                        self.resetWindow()
                        client.mark(self.currentDisplayText())
                        return true
                    }
                }
            default:
                log.debug("key is special key")
        }

        if (!self.isEdited()) {
            return false
        }
        
        if (self.isManualMode()) {
            switch event.keyCode.representative {
                case .enter:
                    fallthrough
                case .space:
                    fallthrough
                case .punctuation:
                    fallthrough
                case .arrow:
                    fallthrough
                case .tab:
                    _ = self.commitCurrent();
                    self.candidateViewModel.reset()
                    return false;
                case .backspace:
                    self.candidateViewModel.handleBackspace()
                case .escape:
                    self.reset()
                    client.clearMarkedText()
                    return true
                default:
                    log.debug("Event not handled")
                    self.reset()
                    return false
            }
        } else {
            switch event.keyCode.representative {
                case .enter:
                    let committed = self.commitCurrent()
                    return committed
                case .backspace:
                    self.candidateViewModel.handleBackspace()
                case .escape:
                    self.reset()
                    client.clearMarkedText()
                    return true
                case .space:
                    self.candidateViewModel.handleSpace(modifiers.contains(.shift))
                case .tab:
                    self.candidateViewModel.handleTab(modifiers.contains(.shift))
                case .arrow(Direction.up):
                    self.candidateViewModel.handleArrowUp()
                case .arrow(Direction.down):
                    self.candidateViewModel.handleArrowDown()                
                default:
                    log.debug("Event not handled")
                    self.reset()
                    return false
            }
        }
        if (self.isClassicMode() && self.isCommited()) {
            client.insert(self.getCommitedText());
            self.resetWindow()
            client.mark(self.currentDisplayText())
        } else if (self.isEdited()) {
            self.resetWindow()
            client.mark(self.currentDisplayText())
        } else {
            self.reset()
            client.clearMarkedText()
        }
        return true 
    }
}
