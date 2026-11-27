#pragma once
//
// Minimal ATL compatibility shim.
//
// The upstream Sandboxie-Plus sources use ATL's CComPtr / CComHeapPtr in a few
// places.  This build environment has no ATL (Visual Studio BuildTools without
// the ATL component), so this header provides just enough of the API to keep
// the sources compiling unchanged: reference-counted COM smart pointers with
// implicit conversion, operator->, operator& (for out-parameters) and
// CoCreateInstance.
//
// Only what the Sandboxie-Plus sources actually use is implemented.
//

#include <windows.h>
#include <unknwn.h>
#include <objbase.h>
#include <combaseapi.h>

template <class T>
class CComPtr
{
public:
    T* p;

    CComPtr() throw() : p(NULL) {}
    CComPtr(T* lp) throw() : p(lp) { if (p != NULL) p->AddRef(); }
    CComPtr(const CComPtr<T>& lp) throw() : p(lp.p) { if (p != NULL) p->AddRef(); }
    ~CComPtr() throw() { if (p != NULL) p->Release(); }

    void Release() throw()
    {
        T* pTemp = p;
        if (pTemp != NULL) { p = NULL; pTemp->Release(); }
    }

    void Attach(T* lp) throw()
    {
        if (p != NULL) p->Release();
        p = lp;
    }

    T* Detach() throw() { T* pTemp = p; p = NULL; return pTemp; }

    operator T*() const throw() { return p; }
    T* operator->() const throw() { return p; }
    T** operator&() throw() { return &p; }
    T* const* operator&() const throw() { return &p; }
    bool operator!() const throw() { return (p == NULL); }

    CComPtr<T>& operator=(T* lp) throw()
    {
        if (p != lp) {
            if (lp != NULL) lp->AddRef();
            if (p != NULL) p->Release();
            p = lp;
        }
        return *this;
    }

    CComPtr<T>& operator=(const CComPtr<T>& lp) throw() { return operator=(lp.p); }

    HRESULT CoCreateInstance(REFCLSID rclsid, LPUNKNOWN pUnkOuter = NULL,
                             DWORD dwClsContext = CLSCTX_INPROC_SERVER) throw()
    {
        if (p != NULL) { p->Release(); p = NULL; }
        return ::CoCreateInstance(rclsid, pUnkOuter, dwClsContext, __uuidof(T), (void**)&p);
    }

    template <class Q>
    HRESULT QueryInterface(CComPtr<Q>& q) const throw()
    {
        if (p == NULL) return E_POINTER;
        return p->QueryInterface(__uuidof(Q), (void**)&q.p);
    }
};

// Heap-allocated COM memory owner (freed with CoTaskMemFree), mirroring ATL's
// CComHeapPtr.  Used for ITEMIDLIST trees returned by the shell.
template <class T>
class CComHeapPtr
{
public:
    T* m_pData;

    CComHeapPtr() throw() : m_pData(NULL) {}
    CComHeapPtr(T* p) throw() : m_pData(p) {}
    CComHeapPtr(const CComHeapPtr<T>& p) throw() : m_pData(p.m_pData) {}
    ~CComHeapPtr() throw() { Free(); }

    void Free() throw()
    {
        if (m_pData != NULL) { CoTaskMemFree(m_pData); m_pData = NULL; }
    }

    void Attach(T* p) throw() { Free(); m_pData = p; }
    T* Detach() throw() { T* p = m_pData; m_pData = NULL; return p; }

    operator T*() const throw() { return m_pData; }
    T* operator->() const throw() { return m_pData; }
    T** operator&() throw() { return &m_pData; }
    bool operator!() const throw() { return (m_pData == NULL); }

    CComHeapPtr<T>& operator=(T* p) throw() { Free(); m_pData = p; return *this; }
    CComHeapPtr<T>& operator=(const CComHeapPtr<T>& p) throw()
    {
        if (this != &p) { Free(); m_pData = p.m_pData; }
        return *this;
    }
};
