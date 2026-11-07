"""Read-only integration check: registration must retain every Windows capability."""
import ctypes,winreg
capabilities=r'Software\Clients\StartMenuInternet\Slim Browser\Capabilities'
registration_root=None
for candidate in (winreg.HKEY_CURRENT_USER,winreg.HKEY_LOCAL_MACHINE):
 try:
  with winreg.OpenKey(candidate,r'Software\RegisteredApplications',0,winreg.KEY_READ|winreg.KEY_WOW64_64KEY) as key:
   if winreg.QueryValueEx(key,'Slim Browser')[0]==capabilities:
    registration_root=candidate
    break
 except FileNotFoundError:
  pass
assert registration_root is not None,'Slim Browser must be registered before this integration check'
checks={
 capabilities: {'ApplicationName':'Slim Browser','ApplicationDescription':None,'ApplicationIcon':None},
 capabilities+r'\URLAssociations':{'http':'SlimBrowser.Url','https':'SlimBrowser.Url'},
 capabilities+r'\FileAssociations':{'.htm':'SlimBrowser.Html','.html':'SlimBrowser.Html'},
 r'Software\Classes\SlimBrowser.Html':{'':'Slim Browser HTML'},
 r'Software\Classes\SlimBrowser.Html\Application':{'ApplicationName':'Slim Browser'},
 r'Software\Classes\SlimBrowser.Url':{'URL Protocol':''},
 r'Software\Classes\SlimBrowser.Html\shell\open\command':{'':None},
}
for path,values in checks.items():
 with winreg.OpenKey(registration_root,path,0,winreg.KEY_READ|winreg.KEY_WOW64_64KEY) as key:
  for name,expected in values.items():
   actual=winreg.QueryValueEx(key,name)[0]
   assert actual==expected if expected is not None else bool(actual),(path,name,actual)
# Windows resolves our handler to an executable, independently of current defaults.
size=ctypes.c_ulong(32768);buf=ctypes.create_unicode_buffer(size.value)
result=ctypes.windll.shlwapi.AssocQueryStringW(0,2,'SlimBrowser.Html','open',buf,ctypes.byref(size))
assert result==0,(result,buf.value)
assert 'slimbrowser' in buf.value.lower(),buf.value
print('PASS all HTTP/HTTPS/HTML capabilities retained; executable resolves:',buf.value)

size=ctypes.c_ulong(32768);buf=ctypes.create_unicode_buffer(size.value)
result=ctypes.windll.shlwapi.AssocQueryStringW(0,4,'SlimBrowser.Html','open',buf,ctypes.byref(size))
assert result==0 and buf.value=='Slim Browser',(result,buf.value)
print('PASS Shell names the app Slim Browser')
