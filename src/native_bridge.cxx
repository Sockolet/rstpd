#include "ILoader.h"

extern "C" void rstpd_document_release(void *document) noexcept {
    static_cast<Scintilla::IDocumentEditable *>(document)->Release();
}
