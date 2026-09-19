#include "ILoader.h"

extern "C" void rstpad_document_release(void *document) noexcept {
    static_cast<Scintilla::IDocumentEditable *>(document)->Release();
}
