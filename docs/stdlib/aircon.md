# aircon

## aircon-create
**Signature:** `(image : String) (mem-kb : i64) (cpu-ms : i64) -> _`
**Intent:** Create a container from image with memory and CPU limits

---

## aircon-start
**Signature:** `(id : i64) -> _`
**Intent:** Start a previously created container

---

## aircon-stop
**Signature:** `(id : i64) -> _`
**Intent:** Stop a running container

---

## aircon-status
**Signature:** `(id : i64) -> _`
**Intent:** Query container status: created, running, stopped, or failed

---

## aircon-list
**Signature:** ` -> _`
**Intent:** List all container IDs

---

