"""Read-only integration check: registration must retain every Windows capability."""
import ctypes,winreg
checks={
 r'Software\Clients\StartMenuInternet\SlimBrowser\Capabilities': {'ApplicationName':'Slim Browser','ApplicationDescription':None,'ApplicationIcon':None},
 r'Software\Clients\StartMenuInternet\SlimBrowser\Capabilities\URLAssociations':{'http':'SlimBrowserURL','https':'SlimBrowserURL'},
 r'Software\Clients\StartMenuInternet\SlimBrowser\Capabilities\FileAssociations':{'.htm':'SlimBrowserHTML','.html':'SlimBrowserHTML'},
 r'Software\Classes\SlimBrowserHTML':{'':'Slim Browser HTML','URL Protocol':'','FriendlyTypeName':'Slim Browser'},
 r'Software\Classes\SlimBrowserHTML\shell\open\command':{'':None},
}
for path,values in checks.items():
 with winreg.OpenKey(winreg.HKEY_CURRENT_USER,path) as key:
  for name,expected in values.items():
   actual=winreg.QueryValueEx(key,name)[0]
   assert actual==expected if expected is not None else bool(actual),(path,name,actual)
# Windows resolves our handler to an executable, independently of current defaults.
size=ctypes.c_ulong(32768);buf=ctypes.create_unicode_buffer(size.value)
result=ctypes.windll.shlwapi.AssocQueryStringW(0,2,'SlimBrowserHTML','open',buf,ctypes.byref(size))
assert result==0,(result,buf.value)
assert 'slimbrowser' in buf.value.lower(),buf.value
print('PASS all HTTP/HTTPS/HTML capabilities retained; executable resolves:',buf.value)

size=ctypes.c_ulong(32768);buf=ctypes.create_unicode_buffer(size.value)
result=ctypes.windll.shlwapi.AssocQueryStringW(0,4,'SlimBrowserHTML','open',buf,ctypes.byref(size))
assert result==0 and buf.value=='Slim Browser',(result,buf.value)
print('PASS Shell names the app Slim Browser')
