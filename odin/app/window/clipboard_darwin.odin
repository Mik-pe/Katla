#+build darwin, arm64
//! Native clipboard data is borrowed only until the next provider read; retained UI clones it.
package window
import NS "core:sys/darwin/Foundation"
import ui "../../ui"
import "core:strings"

@(private="package")
pasteboard_read :: proc(state:rawptr)->string {
    owner:=cast(^Window)state
    pasteboard:=send(NS.id,cast(^NS.Object)NS.objc_lookUpClass("NSPasteboard"),"generalPasteboard")
    kind:=NS.String.alloc()->initWithOdinString("public.utf8-plain-text"); defer kind->release()
    text:=send(^NS.String,cast(^NS.Object)pasteboard,"stringForType:",kind)
    next:=""; if text!=nil { next=strings.clone(string(text->UTF8String())) }
    delete(owner.clipboard); owner.clipboard=next; return next
}
@(private="package")
pasteboard_write :: proc(_:rawptr,value:string) {
    pasteboard:=send(NS.id,cast(^NS.Object)NS.objc_lookUpClass("NSPasteboard"),"generalPasteboard")
    kind:=NS.String.alloc()->initWithOdinString("public.utf8-plain-text"); defer kind->release()
    text:=NS.String.alloc()->initWithOdinString(value); defer text->release()
    send(NS.Integer,cast(^NS.Object)pasteboard,"clearContents")
    send(NS.BOOL,cast(^NS.Object)pasteboard,"setString:forType:",text,kind)
}
clipboard_provider :: proc(owner:^Window)->ui.Clipboard_Provider { return {owner,pasteboard_read,pasteboard_write} }
